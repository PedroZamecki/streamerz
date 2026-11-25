"""Test the isolation gates without starting Streamerz or contacting another instance."""
import io
import json
import os
import socket
import struct
import unittest
from unittest.mock import Mock, patch

import harness as smoke


class InstanceIsolation(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "Unix listeners reuse addresses after their connections close")
    def test_closed_connection_does_not_block_the_next_isolated_run(self):
        with socket.socket() as server:
            server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            server.bind(("127.0.0.1", 0))
            port = server.getsockname()[1]
            server.listen()
            with socket.create_connection(("127.0.0.1", port)) as client:
                connection, _ = server.accept()
                connection.shutdown(socket.SHUT_WR)
                connection.close()
                self.assertEqual(client.recv(1), b"")
        smoke.require_free_port(port)

    def test_occupied_port_is_refused_without_contacting_it(self):
        with socket.socket() as other:
            other.bind(("127.0.0.1", 0))
            other.listen()
            with self.assertRaises(RuntimeError):
                smoke.require_free_port(other.getsockname()[1])
            other.setblocking(False)
            with self.assertRaises(BlockingIOError):
                other.accept()

    def test_only_the_expected_hello_is_accepted(self):
        expected = "Streamerz smoke isolated-random-id"
        smoke.verify_instance({"request": "Hello", "info": {"name": expected}}, expected)
        for hello in ([], {}, {"request": "Hello"}, {"request": "Hello", "info": None},
                      {"request": "Hello", "info": {"name": "Streamerz"}},
                      {"request": "GetInfo", "info": {"name": expected}}):
            with self.subTest(hello=hello), self.assertRaises(RuntimeError):
                smoke.verify_instance(hello, expected)


class WebSocketReads(unittest.TestCase):
    upgrade = b"HTTP/1.1 101 Switching Protocols\r\n\r\n"

    def connect(self, data):
        # Model TCP splitting a header, length or payload across separate recv calls.
        stream = io.BytesIO(data)
        sock = Mock()
        sock.recv.side_effect = lambda size: stream.read(min(size, 1))
        return sock, patch.object(smoke.socket, "create_connection", return_value=sock)

    def test_fragmented_frame_header_and_extended_length(self):
        hello = {"request": "Hello", "info": {"name": "x" * 126}}
        payload = json.dumps(hello).encode()
        sock, connect = self.connect(self.upgrade + struct.pack(">BBH", 0x81, 126, len(payload)) + payload)
        with connect:
            self.assertEqual(smoke.Ws(expected_name=hello["info"]["name"]).hello, hello)
        sock.close.assert_not_called()

    def test_eof_in_upgrade_frame_header_and_payload(self):
        for data in (b"HTTP/1.1", self.upgrade + b"\x81", self.upgrade + b"\x81\x05hi"):
            with self.subTest(data=data):
                sock, connect = self.connect(data)
                with connect, self.assertRaisesRegex(ConnectionError, "response was complete"):
                    smoke.Ws()
                sock.close.assert_called_once()

    def test_failed_upgrade_closes_the_socket(self):
        sock, connect = self.connect(b"HTTP/1.1 404 Not Found\r\n\r\n")
        with connect, self.assertRaisesRegex(ConnectionError, "upgrade was refused"):
            smoke.Ws()
        sock.close.assert_called_once()

    def test_large_request_uses_masked_64_bit_length(self):
        sock, connect = self.connect(self.upgrade + b'\x81\x02{}' * 2)
        request = {"value": "x" * 65_536}
        with connect:
            self.assertEqual(smoke.Ws().ask(request), {})
        data = sock.sendall.call_args.args[0]
        self.assertEqual(data[:2], b"\x81\xff")
        self.assertEqual(struct.unpack(">Q", data[2:10])[0], len(data) - 14)
        mask = data[10:14]
        payload = bytes(value ^ mask[i % 4] for i, value in enumerate(data[14:]))
        self.assertEqual(json.loads(payload), request)
