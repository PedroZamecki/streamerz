"""Test the isolation gates without starting Streamerz or contacting another instance."""
import socket
import unittest

import smoke


class InstanceIsolation(unittest.TestCase):
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
