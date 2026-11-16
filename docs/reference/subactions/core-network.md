# Core / Network (sub-actions)

Sub-actions that talk to the network: UDP broadcasts and HTTP requests.

## UDP Broadcast

- Kind: sub-action
- Code: 2
- Type: UdpBroadcast
- Category: Core/Network
- Needs: nothing

Sends the payload as a UDP broadcast to `255.255.255.255` on the port.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Port | `port` | The port to send to, as a whole number. |
| Payload | `payload` | The text to send. Several lines are allowed. |

### Arguments

None set.

### Notes

- Variables in the payload are filled in first.
- A send that fails is logged as an error.

## Fetch URL

- Kind: sub-action
- Code: 1007
- Type: FetchUrl
- Category: Core/Network
- Needs: nothing

Calls a URL (a GET by default) and puts the answer in a variable.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| URL | `url` | The address to get. Variables are filled in first. |
| Variable name | `variableName` | The variable that gets the body, or the JSON values. |
| Headers (Name=value) | `headers` | Extra request headers. Their values have variables filled in. |
| Parse as JSON | `parseAsJson` | On: read the body as JSON and split it into values. |
| Auto type | `autoType` | On: a value that looks like a number or a boolean is typed as one. |
| Method | `method` | GET, POST, PUT, PATCH or DELETE. Streamerz only: Streamer.bot always does a GET. |
| Body | `body` | What a POST, PUT or PATCH sends. Variables are filled in first. Streamerz only. |
| Content type | `contentType` | The body's type. Empty: `application/json` when the body is JSON, else `text/plain`. A `Content-Type` header wins. Streamerz only. |

### Arguments

- Without Parse as JSON, the variable named by `variableName` holds the body as text.
- `httpStatus` (the answer's number, 0 when there was none) and `httpOk` (true for a 2xx) are always set.
- With Parse as JSON, each value goes to the variable as `variable.key`. Nested values go to `variable.key.inner`, and list items to `variable.list[0]`. A null is set as an empty text.

### Notes

- Only a 2xx answer counts. Any other status is logged as an error. A GET then sets nothing but `httpStatus` and `httpOk`; with another method the answer's body is still set, so an API's error message can be read.
- With Parse as JSON on, a body that isn't JSON is logged as an error, and nothing is set.
