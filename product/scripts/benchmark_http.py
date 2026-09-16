"""Small loopback HTTP/1 fixture and client. Only synthetic benchmark payloads."""
import asyncio
import collections
import contextlib
import json
import math
import socket
import struct
import time


def encode(value):
    return json.dumps(value, separators=(",", ":")).encode()


class Client:
    def __init__(self, port):
        self.port = port
        self.reader = self.writer = None

    async def close(self, reset=False):
        if self.writer:
            if reset:
                with contextlib.suppress(OSError):
                    self.writer.get_extra_info("socket").setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
                self.writer.transport.abort()
            else:
                self.writer.close()
                with contextlib.suppress(OSError):
                    await self.writer.wait_closed()
            self.reader = self.writer = None

    async def exchange(self, method, path, body, headers):
        if self.writer is None:
            self.reader, self.writer = await asyncio.open_connection("127.0.0.1", self.port)
        fields = {"Host": f"127.0.0.1:{self.port}", "Content-Length": str(len(body)), "Content-Type": "application/json", "Connection": "keep-alive", **headers}
        request = (f"{method} {path} HTTP/1.1\r\n"+"".join(f"{k}: {v}\r\n" for k,v in fields.items())+"\r\n").encode()+body
        self.writer.write(request)
        await self.writer.drain()
        first = await self.reader.readexactly(1)
        first_http = time.monotonic()
        head = first+await self.reader.readuntil(b"\r\n\r\n")
        lines = head.decode("ascii").split("\r\n")
        status = int(lines[0].split()[1])
        fields = {}
        response_headers = []
        for line in lines[1:]:
            if line:
                k,v = line.split(":",1); fields[k.lower()] = v.strip()
                response_headers.append([k.lower(), v.strip(" \t")])
        result = dict(status=status, headers=response_headers, first_http_s=first_http, first_body_s=None, first_output_delta_s=None, terminal_marker_s=None, output="", body=b"", usage={"status": "missing"})
        sse_pending = b""
        def observe(chunk):
            nonlocal sse_pending
            if not chunk: return
            now = time.monotonic()
            if result["first_body_s"] is None: result["first_body_s"] = now
            result["body"] += chunk
            if len(result["body"]) > 1024*1024: raise ValueError("fixture response exceeds 1MiB bound")
            if "text/event-stream" not in fields.get("content-type", ""): return
            sse_pending += chunk
            while b"\n\n" in sse_pending:
                frame, sse_pending = sse_pending.split(b"\n\n",1)
                if not frame.startswith(b"data: "): continue
                data = frame[6:]
                if data == b"[DONE]": result["terminal_marker_s"] = now; continue
                event = json.loads(data)
                # OpenAI-compatible streams may send usage:null before the final usage object.
                if event.get("usage") is not None:
                    usage = event["usage"]
                    if result["terminal_marker_s"] is not None:
                        result["usage"] = {"status": "invalid", "reason": "usage_after_terminal"}
                    elif result["usage"]["status"] != "missing":
                        result["usage"] = {"status": "invalid", "reason": "duplicate_usage"}
                    elif not isinstance(usage, dict):
                        result["usage"] = {"status": "invalid", "reason": "usage_not_object"}
                    else:
                        prompt = usage.get("prompt_tokens")
                        completion = usage.get("completion_tokens")
                        if not all(type(value) is int and 0 <= value <= 2**64 - 1 for value in (prompt, completion)):
                            result["usage"] = {"status": "invalid", "reason": "usage_tokens_not_u64"}
                        else:
                            result["usage"] = {"status": "observed", "prompt_tokens": prompt, "completion_tokens": completion}
                for choice in event.get("choices", []):
                    content = choice.get("delta", {}).get("content")
                    if content:
                        if result["first_output_delta_s"] is None: result["first_output_delta_s"] = now
                        result["output"] += content
        if fields.get("transfer-encoding", "").lower() == "chunked":
            while True:
                length = int((await self.reader.readline()).split(b";",1)[0],16)
                if not length:
                    trailer_bytes = 0
                    for _ in range(128):
                        trailer = await self.reader.readline()
                        if not trailer: raise asyncio.IncompleteReadError(b"", 2)
                        trailer_bytes += len(trailer)
                        if trailer_bytes > 32768: raise ValueError("fixture trailer exceeds bound")
                        if trailer == b"\r\n": break
                    else: raise ValueError("too many fixture trailer fields")
                    break
                observe(await self.reader.readexactly(length))
                if await self.reader.readexactly(2) != b"\r\n": raise ValueError("invalid chunk framing")
        elif "content-length" in fields:
            remaining = int(fields["content-length"])
            while remaining:
                chunk = await self.reader.read(min(remaining,16384))
                if not chunk: raise asyncio.IncompleteReadError(b"", remaining)
                remaining -= len(chunk); observe(chunk)
        else:
            raise ValueError("fixture must delimit HTTP response body")
        result["body_eof_s"] = time.monotonic()
        return result


class Mock:
    def __init__(self, quota):
        self.quota = quota
        self.attempts = []
        self.budget_samples = []
        self.plans = {}
        self.debits = collections.deque()
        self.ordinals = collections.Counter()
        self.tasks = set()
        self.writers = set()
        self.origin = time.monotonic()
        self.server = None

    async def start(self):
        self.server = await asyncio.start_server(self.handle, "127.0.0.1", 0)
        self.port = self.server.sockets[0].getsockname()[1]

    def budget(self):
        now = time.monotonic()
        while self.debits and self.debits[0][0]+60 <= now: self.debits.popleft()
        rpm = len(self.debits); tpm = sum(cost for _,cost in self.debits)
        return {"elapsed_s": now-self.origin, "rpm_used": rpm, "tpm_used_fixture_units": tpm,
                "rpm_unused": max(0,self.quota["rpm"]-rpm) if self.quota else None,
                "tpm_unused_fixture_units": max(0,self.quota["tpm"]-tpm) if self.quota else None}

    async def handle(self, reader, writer):
        self.writers.add(writer)
        try:
            while True:
                try:
                    head = await reader.readuntil(b"\r\n\r\n")
                except (asyncio.IncompleteReadError, ConnectionError): break
                lines = head.decode("ascii").split("\r\n")
                headers = dict(line.lower().split(":",1) for line in lines[1:] if line)
                body = await reader.readexactly(int(headers.get("content-length", "0")))
                ingress = headers.get("x-benchmark-ingress", "").strip()
                if ingress not in self.plans: raise ValueError("unknown fixture ingress")
                plan = self.plans[ingress]
                now = time.monotonic()
                self.ordinals[ingress] += 1
                task = asyncio.current_task(); self.tasks.add(task)
                payload = json.loads(body) if body else {}
                metadata = plan.get("metadata", False)
                estimated = 0 if metadata else len(body)+payload["max_tokens"]
                input_units = 0 if metadata else math.ceil(len(body)*plan["actual_ratio"])
                output_units = 0 if metadata else max(1,math.ceil(payload["max_tokens"]*plan["actual_ratio"]))
                actual = input_units+output_units
                attempt = {"id": f"{ingress}/{self.ordinals[ingress]}", "ingress_id": ingress,
                           "received_s": now, "estimated_cost": estimated, "actual_cost_fixture_units": actual,
                           "body_bytes": len(body), "outcome": "pending"}
                self.attempts.append(attempt)
                budget = self.budget()
                accepted = self.quota is None or (budget["rpm_used"] < self.quota["rpm"] and budget["tpm_used_fixture_units"]+actual <= self.quota["tpm"])
                try:
                    if not accepted:
                        delay = max(0.001, self.debits[0][0]+60-now) if self.debits else 60
                        data = encode({"error": {"type": "rate_limit_error", "code": "rate_limit_exceeded"}})
                        writer.write(f"HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\nretry-after-ms: {math.ceil(delay*1000)}\r\n\r\n".encode()+data)
                        await writer.drain()
                        attempt["outcome"] = "rejected"
                    else:
                        self.debits.append((now,actual))
                        if plan["service_ms"]:
                            await asyncio.sleep(plan["service_ms"]/1000)
                        if writer.is_closing():
                            raise ConnectionError("synthetic downstream closed")
                        attempt["first_write_s"] = time.monotonic()
                        if metadata:
                            data=encode({"object":"list", "data":[{"id":"synthetic"}]})
                            writer.write(f"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n\r\n".encode()+data)
                        else:
                            writer.write(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
                            events = [{"choices":[{"delta":{"role":"assistant"}}]},
                                      {"choices":[{"delta":{"content":"fixture:"+ingress}}]},
                                      {"choices":[],"usage":{"prompt_tokens":input_units,"completion_tokens":output_units}}]
                            for value in events:
                                frame=b"data: "+encode(value)+b"\n\n"
                                writer.write(f"{len(frame):x}\r\n".encode()+frame+b"\r\n")
                            end=b"data: [DONE]\n\n"
                            writer.write(f"{len(end):x}\r\n".encode()+end+b"\r\n0\r\n\r\n")
                        await writer.drain()
                        attempt["outcome"]="completed"
                except (ConnectionError, BrokenPipeError):
                    attempt["outcome"]="disconnected"
                finally:
                    attempt["ended_s"]=time.monotonic()
                    self.tasks.discard(task)
                    self.budget_samples.append(self.budget())
        except (OSError, asyncio.IncompleteReadError):
            pass
        finally:
            self.writers.discard(writer)
            writer.close()
            with contextlib.suppress(OSError): await writer.wait_closed()

    async def drain(self):
        for _ in range(200):
            if not self.tasks: return
            await asyncio.sleep(.01)
        raise RuntimeError("mock attempts remain pending")

    async def close(self):
        self.server.close(); await self.server.wait_closed()
        for writer in list(self.writers): writer.close()
        await self.drain()


async def request_once(row, port, arm, origin, client=None):
    own_client = client is None
    if own_client: client = Client(port)
    start = time.monotonic()
    result = {"id": row["id"], "outcome": "error", "sent_s": start,
              "scheduling_lag_ms": max(0,(start-origin-row["offset_s"])*1000),
              "usage": {"status": "not_observed"}}
    metadata=row.get("metadata",False)
    body = b"" if metadata else encode({"model":"synthetic","messages":[{"role":"user","content":"x"*row["input_bytes"]}],"max_tokens":row["output_reservation"],"stream":True})
    endpoint = "/models" if metadata else "/chat/completions"
    path = "/v1"+endpoint if arm=="direct" else f"/r/r{row['root']}/v1"+endpoint
    headers = {"x-benchmark-ingress": row["id"]}
    cancellation = row.get("cancel_after_ms")
    try:
        response = await asyncio.wait_for(client.exchange("GET" if metadata else "POST",path,body,headers), cancellation/1000 if cancellation else row.get("timeout_s",125))
        result["status"]=response["status"]
        for key in ("first_http", "first_body", "first_output_delta", "terminal_marker", "body_eof"):
            value = response.get(key+"_s")
            result[key+"_ms"] = (value-start)*1000 if value is not None else None
        if response["status"]==200:
            valid = json.loads(response["body"])=={"object":"list", "data":[{"id":"synthetic"}]} if metadata else response["output"]=="fixture:"+row["id"] and response["terminal_marker_s"] is not None
            if not valid: raise ValueError("synthetic payload/terminal mismatch")
            result["outcome"]="completed"
            result["payload_valid"]=True
            result["usage"] = {"status": "not_applicable"} if metadata else response["usage"]
        elif response["status"]==429: result["outcome"]="rejected"
        elif response["status"] in (408,504): result["outcome"]="timeout"
        else: result["outcome"]="error"
    except TimeoutError:
        result["outcome"]="cancelled" if cancellation else "timeout"
        await client.close(reset=True)
    except (OSError, ValueError, asyncio.IncompleteReadError) as error:
        result["error_class"] = type(error).__name__
        await client.close(reset=True)
    finally:
        result["ended_s"]=time.monotonic()
        result["elapsed_ms"]=(result["ended_s"]-start)*1000
        if own_client: await client.close()
    return result
