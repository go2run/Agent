#!/usr/bin/env python3
"""
HTTP server with Cross-Origin Isolation headers.

Required for @wasmer/sdk which uses SharedArrayBuffer via Web Workers.
Headers added:
  - Cross-Origin-Opener-Policy: same-origin
  - Cross-Origin-Embedder-Policy: require-corp

Usage:
  python3 serve.py [port] [bind_address]
  python3 serve.py 8080
  python3 serve.py 3000 0.0.0.0
"""

import http.server
import sys


class COOPCOEPHandler(http.server.SimpleHTTPRequestHandler):
    """HTTP handler that adds cross-origin isolation headers to every response."""

    def end_headers(self):
        # Required for SharedArrayBuffer (used by @wasmer/sdk)
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        # Correct MIME type for .wasm files
        super().end_headers()

    def guess_type(self, path):
        """Ensure correct MIME types for WASM and JS modules."""
        if path.endswith('.wasm'):
            return 'application/wasm'
        if path.endswith('.mjs'):
            return 'application/javascript'
        return super().guess_type(path)


if __name__ == '__main__':
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
    bind = sys.argv[2] if len(sys.argv) > 2 else '127.0.0.1'

    server = http.server.HTTPServer((bind, port), COOPCOEPHandler)
    print(f'Serving on http://{bind}:{port} with COOP/COEP headers')
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print('\nShutting down.')
        server.shutdown()
