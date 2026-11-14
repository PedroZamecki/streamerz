"""Test the isolation gates without starting Streamerz or contacting another instance."""
import socket
import os
import unittest

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
