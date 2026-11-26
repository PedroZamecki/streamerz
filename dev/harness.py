"""Shared helpers for the Smoke and capture scripts: the dev instance's WebSocket client and the
port and instance checks that keep a test from driving an instance that is not its own."""
import base64, json, os, socket, struct

PORT = int(os.environ.get("STREAMERZ_SMOKE_PORT", "8095"))
env = dict(os.environ, DISPLAY=os.environ.get("DISPLAY", ":99"), STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(PORT))
env.pop("WAYLAND_DISPLAY", None)


class Ws:
    def __init__(self, expected_name=None):
        self.s = socket.create_connection(("127.0.0.1", PORT), timeout=15)
        try:
            key = base64.b64encode(os.urandom(16)).decode()
            self.s.sendall(f"GET / HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
            headers = bytearray()
            while not headers.endswith(b"\r\n\r\n"):
                if len(headers) == 16_384:
                    raise ConnectionError("WebSocket handshake headers are too large")
                headers.extend(self._read_exactly(1))
            if headers.split(b"\r\n", 1)[0].split()[1:2] != [b"101"]:
                raise ConnectionError("WebSocket upgrade was refused")
            self.hello = self.read()
            if expected_name is not None:
                verify_instance(self.hello, expected_name)
        except BaseException:
            self.s.close()
            raise

    def _read_exactly(self, size):
        data = bytearray()
        while len(data) < size:
            part = self.s.recv(min(size - len(data), 65_536))
            if not part:
                raise ConnectionError("WebSocket closed before the response was complete")
            data.extend(part)
        return data

    def read(self):
        h = self._read_exactly(2)
        n = h[1] & 127
        if n == 126:
            n = struct.unpack(">H", self._read_exactly(2))[0]
        elif n == 127:
            n = struct.unpack(">Q", self._read_exactly(8))[0]
        return json.loads(self._read_exactly(n))

    def ask(self, req):
        data = json.dumps(req).encode()
        mask = os.urandom(4)
        size = len(data)
        if size < 126:
            hdr = bytes([0x81, 0x80 | size])
        elif size < 65_536:
            hdr = bytes([0x81, 0x80 | 126]) + struct.pack(">H", size)
        else:
            hdr = bytes([0x81, 0x80 | 127]) + struct.pack(">Q", size)
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
        return self.read()


def require_free_port(port):
    with socket.socket() as probe:
        if os.name != "nt":
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            probe.bind(("127.0.0.1", port))
        except OSError as error:
            raise RuntimeError(f"test port {port} is unavailable; no instance was contacted") from error


def verify_instance(hello, expected_name):
    info = hello.get("info") if isinstance(hello, dict) else None
    if not isinstance(info, dict) or hello.get("request") != "Hello" or info.get("name") != expected_name:
        raise RuntimeError("refusing to drive a WebSocket instance that does not belong to this test")

