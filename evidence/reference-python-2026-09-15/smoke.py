"""Bounded local wire smoke; this is not a performance benchmark."""
import argparse, json, os, signal, subprocess, threading, time, urllib.request, urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
ROOT = Path(__file__).resolve().parent
CACHE = Path("/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/python")
observed = []
class Mock(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *args): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
        reject = body.get("messages", [{}])[0].get("content") == "reject"
        stream = body.get("stream", False)
        observed.append({"path": self.path, "model": body.get("model"), "stream": stream, "reject": reject, "ingress_header": self.headers.get("x-benchmark-ingress"), "agent_header": self.headers.get("x-hivemind-agent-id")})
        if reject:
            data = json.dumps({"error": {"message": "synthetic quota", "type": "rate_limit_error", "code": "rate_limit_exceeded"}}).encode()
            self.send_response(429); self.send_header("Retry-After", "1")
            self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data); return
        usage = {"prompt_tokens": 8, "completion_tokens": 2, "total_tokens": 10}
        data = {"id": "chatcmpl-loopback", "object": "chat.completion", "created": 1, "model": body.get("model"), "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}], "usage": usage}
        if stream:
            chunks = [dict(data, object="chat.completion.chunk", choices=[{"index": 0, "delta": {"role": "assistant", "content": "ok"}, "finish_reason": None}], usage=None), dict(data, object="chat.completion.chunk", choices=[{"index": 0, "delta": {}, "finish_reason": "stop"}])]
            wire = b"".join(b"data: " + json.dumps(c).encode() + b"\n\n" for c in chunks) + b"data: [DONE]\n\n"
            self.send_response(200); self.send_header("Content-Type", "text/event-stream"); self.send_header("Content-Length", str(len(wire))); self.end_headers(); self.wfile.write(wire); return
        wire = json.dumps(data).encode(); self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(wire))); self.end_headers(); self.wfile.write(wire)
    def do_GET(self):
        wire = b'{"object":"list","data":[{"id":"fixture","object":"model","owned_by":"mock"}]}'
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length",str(len(wire))); self.end_headers(); self.wfile.write(wire)
def main():
    parser=argparse.ArgumentParser();parser.add_argument("arm",choices=["hivemind-head","hivemind-stable","litellm"]);args=parser.parse_args()
    env = {k:v for k,v in os.environ.items() if k in ["PATH","HOME","LANG","LC_ALL","TMPDIR","SYSTEMROOT"]}
    env.update({"NO_PROXY":"127.0.0.1,localhost", "LITELLM_LOCAL_MODEL_COST_MAP":"True", "DO_NOT_TRACK":"1", "OTEL_SDK_DISABLED":"true", "HF_HUB_OFFLINE":"1", "TRANSFORMERS_OFFLINE":"1", "LITELLM_MODE":"DEV"})
    if args.arm == "hivemind-head":
        cmd=[str(CACHE/"hivemind-head-venv/bin/python"),str(ROOT/"hivemind-head-launch.py")]; port=18765; health="/_health"
    elif args.arm == "hivemind-stable":
        cmd=[str(CACHE/"hivemind-venv/bin/hivemind"),"proxy","--port","18765","--upstream","http://127.0.0.1:18764","--max-concurrency","2","--min-concurrency","2","--max-retries","0"];port=18765;health="/_health"
    else:
        cmd=[str(CACHE/"litellm-venv/bin/python"),str(ROOT/"litellm-launch.py")];port=18766;health="/health/liveliness"
    work=CACHE/(args.arm+"-runtime");work.mkdir(exist_ok=True)
    mock=ThreadingHTTPServer(("127.0.0.1",18764),Mock);threading.Thread(target=mock.serve_forever,daemon=True).start()
    result={"arm":args.arm,"command":cmd,"performance_benchmark":False,"cases":[],"upstream_attempts":observed,"environment_overrides":env | {"HOME":"[inherited]","PATH":"[inherited]"}}
    result["environment_overrides"].pop("TMPDIR",None)
    log=(ROOT/(args.arm+"-startup.log")).open("w")
    proc=subprocess.Popen(cmd,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
    try:
        ready=False
        for _ in range(200):
            if proc.poll() is not None: break
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}{health}",timeout=.5) as r: ready=r.status==200
                if ready: break
            except Exception: time.sleep(.1)
        result["startup_ready"]=ready
        if ready:
            for name,content,stream in [("json","ok",False),("sse","ok",True),("429-no-retry","reject",False)]:
                payload={"model":"synthetic","messages":[{"role":"user","content":content}],"max_tokens":8,"stream":stream}
                request=urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",data=json.dumps(payload).encode(),headers={"Content-Type":"application/json","x-hivemind-agent-id":"smoke", "x-benchmark-ingress":"smoke-ingress"})
                before=len(observed)
                try:
                    response=urllib.request.urlopen(request,timeout=15)
                except urllib.error.HTTPError as e: response=e
                with response:
                    raw=response.read();status=response.status
                    passed=(status==429 if name=="429-no-retry" else status==200 and (b"[DONE]" in raw if stream else json.loads(raw)["choices"][0]["message"]["content"]=="ok"))
                result["cases"].append({"name":name,"status":status,"wire_bytes":len(raw),"upstream_attempts":len(observed)-before,"ingress_forwarded": observed[-1].get("ingress_header") == "smoke-ingress" if len(observed)>before else False, "pass":passed and len(observed)-before==1 and observed[-1].get("ingress_header") == "smoke-ingress"})
            if args.arm.startswith("hivemind"):
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/_stats") as r: result["stats"]=json.load(r)
        result["passed"]=ready and len(result["cases"])==3 and all(c["pass"] for c in result["cases"])
    except Exception as e:
        result["error"]=repr(e);result["passed"]=False
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid,signal.SIGTERM)
            try:proc.wait(timeout=5)
            except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
        result["exit_code"]=proc.returncode
        log.close();mock.shutdown();mock.server_close()
        (ROOT/(args.arm+"-smoke.json")).write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps({k:result.get(k) for k in ["arm","startup_ready","cases","passed","error"]},indent=2))
    raise SystemExit(0 if result.get("passed") else 1)
if __name__ == "__main__": main()
