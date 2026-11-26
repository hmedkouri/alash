# Prints the tool names in the request codex sends. See docs/tools-off.md.
import http.server, json, sys, gzip
try:
    import zstandard
except ImportError:
    zstandard = None
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        self.send_response(404); self.end_headers()
    def do_POST(self):
        b = self.rfile.read(int(self.headers.get('content-length', 0)))
        enc = self.headers.get('content-encoding', '')
        if enc == 'gzip': b = gzip.decompress(b)
        elif enc == 'zstd' and zstandard: b = zstandard.ZstdDecompressor().decompressobj().decompress(b)
        try:
            d = json.loads(b)
            names = [t.get('name') or t.get('type') for t in d.get('tools', [])]
            ins = d.get('instructions') or ''
            with open(sys.argv[2], 'a') as f:
                f.write(json.dumps({'path': self.path, 'tools': names, 'pineapple_in_body': 'PINEAPPLE' in b.decode(errors='replace'), 'instr_len': len(ins)}) + '\n')
        except Exception as e:
            with open(sys.argv[2], 'a') as f: f.write(f'ERR {enc} {e!r} {b[:80]!r}\n')
        self.send_response(400); self.end_headers(); self.wfile.write(b'{}')
http.server.HTTPServer(('127.0.0.1', int(sys.argv[1])), H).serve_forever()
