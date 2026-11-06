#!/usr/bin/env python3
"""Exercise every advertised MCP tool over stdio against an isolated dev app.

Usage: dev/mcp_smoke.py [target/debug/Streamerz]
Needs the project's hidden display at :99. Uses no credentials or live accounts.
"""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import tempfile
import time


def main():
    binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/Streamerz').resolve())
    with tempfile.TemporaryDirectory(prefix='streamerz-mcp-') as scratch:
        config_home = Path(scratch)
        (config_home / 'streamerz').mkdir()
        with socket.socket() as probe:
            probe.bind(('127.0.0.1', 0))
            port = probe.getsockname()[1]
        instance = 'Streamerz MCP smoke ' + str(os.getpid())
        (config_home / 'streamerz/streamerz.json').write_text(json.dumps({
            'start_hidden': False, 'instance_name': instance, 'locale': 'en_US', 'websockets': {'port': port},
        }))
        env = dict(os.environ, DISPLAY=':99', XDG_CONFIG_HOME=scratch,
                   STREAMERZ_DEV='1', STREAMERZ_DEV_PORT=str(port),
                   STREAMERZ_URL=f'ws://127.0.0.1:{port}/')
        env.pop('WAYLAND_DISPLAY', None)
        env.pop('STREAMERZ_CONFIG', None)
        app = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        mcp = None
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if app.poll() is not None:
                    raise RuntimeError('dev app exited during startup')
                try:
                    with socket.create_connection(('127.0.0.1', port), timeout=.2):
                        break
                except OSError:
                    time.sleep(.1)
            else:
                raise RuntimeError('dev app did not start')
            mcp = subprocess.Popen([binary, '--mcp'], env=env, stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, text=True, encoding="utf-8", bufsize=1)
            sequence = 0
            seen = set()

            def rpc(method, params=None):
                nonlocal sequence
                sequence += 1
                mcp.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': sequence, 'method': method,
                                           'params': params or {}}) + '\n')
                mcp.stdin.flush()
                if not select.select([mcp.stdout], [], [], 20)[0]:
                    raise RuntimeError(f'{method}: no MCP response')
                reply = json.loads(mcp.stdout.readline())
                assert reply['id'] == sequence and 'result' in reply, reply
                return reply['result']

            def call(tool_name, expected_error=False, **args):
                seen.add(tool_name)
                result = rpc('tools/call', {'name': tool_name, 'arguments': args})
                assert result['isError'] == expected_error, (tool_name, result)
                text = result['content'][0]['text']
                return text if expected_error else json.loads(text)

            mcp.stdin.write('{broken JSON\n')
            mcp.stdin.flush()
            assert select.select([mcp.stdout], [], [], 5)[0], 'missing parse error'
            assert json.loads(mcp.stdout.readline())['error']['code'] == -32700
            rpc('initialize', {'protocolVersion': '2025-06-18'})
            rpc('ping')
            names = {tool['name'] for tool in rpc('tools/list')['tools']}
            status = call('status')
            assert status['info']['name'] == instance, status
            for name in ['log', 'connectors', 'list_actions', 'list_action_groups', 'list_plugins',
                         'agents', 'list_templates', 'song_request_template', 'overlays',
                         'list_triggers', 'list_subactions', 'chat_out']:
                call(name)
            call('guide', name='MCP')
            call('docs_search', query='message', limit=2)
            call('describe', kind='subaction', code=10)
            call('docs', kind='subaction', code=10)
            scenario = call('run_scenario', scenario={
                'name': 'MCP smoke', 'actions': [{'name': 'test', 'subActions': [
                    {'type': 1020, 'logLevel': 2, 'message': 'MCP scenario ran'}]}],
                'steps': [{'run': 'test'}], 'expect': {'log': ['MCP scenario ran']}})
            assert scenario['passed'], scenario
            call('request', json={'request': 'GetActions'})
            call('ui', do='windows')
            shot = call('ui', do=f'shot {scratch}/window.png')
            assert (config_home / 'window.png').is_file(), shot
            call('manage_action_group', operation='create', group='MCP test')
            action = call('new_action', name='MCP test action', group='MCP test')
            action['id'] = 'mcp-short-id'
            saved = call('save', what='action', item=action)['saved']
            assert call('get', what='action', id=saved)['item']['name'] == 'MCP test action'
            action['name'] = 'MCP edited action'
            call('save', what='action', item=action)
            assert call('get', what='action', id=saved)['item']['name'] == 'MCP edited action'
            assert len(call('get', what='action')['items']) == 1
            call('run_action', action=saved)
            call('run_action', action=action['name'])
            for kind in ['command', 'timer', 'group', 'queue']:
                item_id = call('save', what=kind, item={'name': 'MCP ' + kind, 'enabled': False})['saved']
                assert call('get', what=kind, id=item_id)['item']['name'] == 'MCP ' + kind
                call('delete', what=kind, id=item_id)
                assert all(item['id'] != item_id for item in call('get', what=kind)['items'])
            call('set_group_variables', group='MCP test', values={'answer': 42})
            assert call('group_variables', group='MCP test')['values']['answer'] == 42
            call('set_group_meta', group='MCP test', meta={'description': 'MCP smoke'})
            assert call('group_meta', group='MCP test')['meta']['description'] == 'MCP smoke'
            call('action_group_icon', group='MCP test', icon=None)
            exported = call('export_actions', ids=[saved])['export']
            call('import_actions', export=exported)
            call('export_group', group='MCP test')
            call('save_group_files', group='MCP test', folder=scratch)
            call('overlay_publish', topic='mcp-test', value={'answer': 42})
            call('fire_trigger', trigger=101)
            call('inject_chat', platform='twitch', user='tester', text='hello')
            runs = call('action_history')['runs']
            assert runs and runs[0]['executionStatus'] == 'completed', runs
            assert call('tiktok_gifts')['state'] == 'unavailable'
            preview = call('preview_gift_trigger',
                           trigger={'giftId': '5655', 'giftDelivery': 1, 'minCoins': 5},
                           gift={'giftId': '5655', 'diamondCount': 1, 'repeatCount': 5,
                                 'giftType': 1, 'repeatEnd': True})
            assert preview['matches'] is True and preview['runsActions'] is False
            call('tiktok_bridge', action='status')
            # Exercise integration failures without starting connections or changing the browser.
            for name, args in [
                ('obs_request', {'requestType': 'GetSceneList'}),
                ('spotify_find', {'query': 'test'}),
                ('spotify_player', {}), ('spotify_queue', {'query': 'test'}),
                ('spotify_playback', {'command': 'pause'}),
                ('plugin_enable', {'plugin': 'nonexistent-mcp-test'}),
                ('agent_setup', {'agent': 'nonexistent-mcp-test'}),
                ('countdown', {'clockId': 'nonexistent-mcp-test'}),
                ('search_channels', {'platform': 'tiktok', 'query': 'test'}),
                ('tiktok_gifts', {'refresh': True}),
            ]:
                call(name, expected_error=True, **args)
            assert call('spotify_current')['track'] is None
            local = rpc('tools/call', {'name': 'spotify_local', 'arguments': {}})
            seen.add('spotify_local')  # OS-dependent availability, both outcomes are valid.
            assert isinstance(local['isError'], bool)
            templates = call('list_templates')['templates']
            call('add_template', name=templates[0]['name'])
            call('delete', what='action', id=saved)
            call('get', expected_error=True, what='action', id=saved)
            call('manage_action_group', operation='rename', group='MCP test', name='MCP renamed')
            call('manage_action_group', operation='delete', group='MCP renamed')
            assert seen == names, f'uncovered tools: {names - seen}; unexpected: {seen - names}'
            assert app.poll() is None, 'app crashed'
            print(f'Passed: {len(seen)} MCP tools, stdio protocol, authoring round trips and integration errors')
        finally:
            for process in [mcp, app]:
                if process is not None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()


if __name__ == '__main__':
    main()
