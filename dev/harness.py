"""Shared helpers for the Smoke and capture scripts: the dev instance's WebSocket client and the
port and instance checks that keep a test from driving an instance that is not its own."""
import base64, json, os, socket, struct

PORT = int(os.environ.get("STREAMERZ_SMOKE_PORT", "8095"))
env = dict(os.environ, DISPLAY=os.environ.get("DISPLAY", ":99"), STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(PORT))
env.pop("WAYLAND_DISPLAY", None)


class Ws:
    def __init__(self, expected_name=None):
        self.s = socket.create_connection(("127.0.0.1", PORT), timeout=15)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(f"GET / HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(1)
        self.hello = self.read()
        if expected_name is not None:
            try:
                verify_instance(self.hello, expected_name)
            except RuntimeError:
                self.s.close()
                raise

    def read(self):
        h = self.s.recv(2)
        n = h[1] & 127
        if n == 126:
            n = struct.unpack(">H", self.s.recv(2))[0]
        elif n == 127:
            n = struct.unpack(">Q", self.s.recv(8))[0]
        data = b""
        while len(data) < n:
            data += self.s.recv(n - len(data))
        return json.loads(data)

    def ask(self, req):
        data = json.dumps(req).encode()
        mask = os.urandom(4)
        hdr = bytes([0x81])
        hdr += bytes([0x80 | len(data)]) if len(data) < 126 else bytes([0x80 | 126]) + struct.pack(">H", len(data))
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

