# Fake Responses endpoint: first turn orders apply_patch to add proof.txt, second turn
# logs what codex sent back as the tool output and ends the conversation.
import http.server, json, sys
LOG = sys.argv[2]
def sse(evs):
    return ''.join(f"event: {e['type']}\ndata: {json.dumps(e)}\n\n" for e in evs).encode()
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self): self.send_response(404); self.end_headers()
    def do_POST(self):
        d = json.loads(self.rfile.read(int(self.headers.get('content-length', 0))))
        outs = [i for i in d.get('input', []) if 'output' in i.get('type', '')]
        with open(LOG, 'a') as f:
            f.write('TOOLS ' + json.dumps([(t.get('type'), t.get('name')) for t in d.get('tools', [])]) + '\n')
            for o in outs: f.write('TOOL_OUTPUT ' + json.dumps(o)[:600] + '\n')
        if not outs:
            ap = next((t for t in d.get('tools', []) if t.get('name') == 'apply_patch'), {})
            patch = "*** Begin Patch\n*** Add File: proof.txt\n+written by the model\n*** End Patch\n"
            if ap.get('type') == 'custom':
                item = {"type": "custom_tool_call", "id": "ctc_1", "call_id": "call_1", "name": "apply_patch", "input": patch, "status": "completed"}
            else:
                item = {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "apply_patch", "arguments": json.dumps({"input": patch}), "status": "completed"}
        else:
            item = {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": "done", "annotations": []}]}
        rid = "resp_%d" % len(outs)
        body = sse([
            {"type": "response.created", "response": {"id": rid}},
            {"type": "response.output_item.done", "output_index": 0, "item": item},
            {"type": "response.completed", "response": {"id": rid, "usage": {"input_tokens": 1, "input_tokens_details": None, "output_tokens": 1, "output_tokens_details": None, "total_tokens": 2}}},
        ])
        self.send_response(200); self.send_header('content-type', 'text/event-stream'); self.send_header('content-length', str(len(body))); self.end_headers(); self.wfile.write(body)
http.server.HTTPServer(('127.0.0.1', int(sys.argv[1])), H).serve_forever()
