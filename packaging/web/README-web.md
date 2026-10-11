# AI Assistant in the browser

This folder is a static website: `index.html`, a JavaScript loader and the app as WebAssembly
(`.wasm`). It needs a web server: browsers refuse to run WebAssembly from a page opened as a local
file (`file://…/index.html` stays blank, showing only a hint).

## Run it locally

In this folder:

```sh
python3 -m http.server 8000 --bind 127.0.0.1
```

Then open <http://127.0.0.1:8000>. If port 8000 is taken (`Address already in use`), pick another
number. Any static server works (`npx serve`, nginx, Caddy, …) as long as it sends `.wasm` files as
`application/wasm`.

## Put it online

Upload the folder's contents to any static host: GitHub Pages, Cloudflare Pages, Netlify, a plain
web server. It works from any URL path. `_headers` sets the content type and caching on Cloudflare
Pages and Netlify; elsewhere, set the same headers in the server's configuration.

## Your settings and API key

Settings and the OpenRouter API key are kept in the browser's local storage **for that address**:
`http://127.0.0.1:8000` and `http://127.0.0.1:8001` are different sites with separate settings.
Anything running on the same site can read the key, so host the app only where you trust every
page, and prefer the desktop app (which keeps the key in the OS keyring) on shared computers.

Requests go straight from the browser to OpenRouter (`https://openrouter.ai`); no server of ours is
involved. The browser needs WebGL2.
