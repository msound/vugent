# Listen beyond localhost with a random access token

## Context and Problem Statement

Vugent listens on `127.0.0.1`, so when it runs on a remote server over SSH, the browser on the user's laptop cannot
reach it. It also has no authentication: listening on a public address as it is would let anyone who can reach the
port read the code, the diffs and the commit history.

Which addresses may vugent listen on, how is access limited to the person who started it, and is plain HTTP good
enough? Issue #31 describes the problem.

## Considered Options

Reaching vugent on a remote server:

* Only SSH port forwarding, with vugent on `127.0.0.1` as today
* A `--host` option, with an access token whenever the address is not loopback

The token:

* Random at every start, never chosen by the user
* Random by default, or fixed with `--token` or an environment variable so that bookmarks survive restarts

Where the token is kept after the first visit:

* An HttpOnly cookie, set when a URL with `?token=` is first opened
* In the URL of every request
* In a header that the frontend adds to every request

Encryption:

* Plain HTTP
* HTTPS with a self-signed certificate

## Decision Outcome

Chosen options: **a `--host` option with a token beyond loopback**, a **random token at every start**, kept in an
**HttpOnly cookie**, over **plain HTTP**.

* SSH port forwarding is the safer way and is documented as such, but it needs a second command or an editor that
  forwards ports, and some users cannot or would rather not set it up. `--host` keeps the simple path possible, and
  the token makes it safe enough to offer.
* Tokens that people pick are often short, guessable and reused elsewhere, and passing one on the command line leaves
  it in shell history and in the process list. A random token avoids all of this. The cost is that a bookmark stops
  working when vugent restarts, which is acceptable since vugent prints the new URL every time.
* A cookie is sent by the browser with every same-origin request, including `fetch` and `EventSource`, so the
  frontend needs no changes, and the Server-Sent Events stream works without custom headers, which `EventSource`
  cannot send. Keeping the token in every URL would leave it in the address bar, history and logs.
* HTTPS with a self-signed certificate makes the browser show a full-page warning that the user must click through,
  and teaches them to ignore it. Plain HTTP is accepted, with a warning; users who need encryption use SSH port
  forwarding.

### The listen address

| | Decision |
|---|---|
| `--host <IP>` | An IPv4 or IPv6 address, `127.0.0.1` by default. The server's own address, `0.0.0.0` or `::` make vugent reachable from other machines. Host names are not taken, so that vugent never listens on an address the user did not expect. |
| Printed URL | For an unspecified address (`0.0.0.0` or `::`), the URL uses the machine's host name, since `http://0.0.0.0:PORT/` cannot be opened from elsewhere. Otherwise it uses the address, with IPv6 in brackets. |
| Browser | Not opened when the address is not loopback: the server is most likely headless, and the browser that should open it is on another machine. |
| `--port` | Its help mentions that a fixed port, such as `--port 8000`, helps with firewalls that only allow some ports, and that ports below 1024 need root. |

### The token

* Generated at every start when the address is not loopback: 32 bytes from the operating system's random number
  generator, written as 64 hex characters. There is no option or environment variable to choose it.
* On loopback no token is needed, as today. Other users of the same machine could reach the port; a token for
  loopback can be added later if that turns out to matter.
* Vugent prints the URL with the token, such as `http://build-box:8000/?token=…`.
* Tokens are compared in constant time.

### Checking requests

* Every request is checked: the page and its assets, the API and the event stream. `/api/health` is no exception.
* A request whose query has a valid `token` gets a `303 See Other` to the same path and query without `token`, and a
  cookie, so the token does not linger in the address bar or history.
* A request with a valid cookie is served.
* Anything else gets `401 Unauthorized`, with a short plain-text message saying to open the URL that vugent printed.
* The cookie is `HttpOnly`, so scripts cannot read it, `SameSite=Lax`, so other sites cannot make `fetch`, frame or
  image requests with it, and has `Path=/` and no expiry, so it lasts until the browser closes. It is not `Secure`,
  since the connection is plain HTTP.
* `SameSite=Strict` was ruled out: when the URL is opened from a link on another site, such as a chat app, the
  redirect belongs to a cross-site navigation, so the browser would not send a strict cookie with it and the first
  visit would fail. `Lax` sends it with top-level navigations, which can only show vugent's own page to the user who
  already has the cookie; every route only reads.
* Cookies are not kept apart by port, so its name includes the port, such as `vugent-token-8000`: two vugents on
  the same machine then do not overwrite each other's cookie.

### Plain HTTP

When the address is not loopback, vugent prints a warning to stderr that the connection is not encrypted, so the
token and the code can be read by anyone on the network between the browser and the server, and suggests SSH port
forwarding instead.

### Consequences

* Good, because vugent can be used on a remote server with a single flag, and nothing is readable without the token.
* Good, because the default stays as simple and as safe as today: loopback, no token, the browser opened.
* Good, because the frontend is unchanged.
* Bad, because the token and the code travel unencrypted when `--host` is used.
* Bad, because a bookmark or an open tab stops working after vugent restarts, until the new URL is opened.
* Bad, because other users of the same machine can still reach vugent on loopback.
