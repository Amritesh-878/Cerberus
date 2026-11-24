// A fake Messages API for recording a real claude session: scripted replies, no network.
const http = require("http");
const fs = require("fs");
const path = require("path");

const PORT = Number(process.env.FAKE_PORT || 8820);
const WORK = process.env.FAKE_WORK || process.cwd();
const LOG = path.join(WORK, "..", "api-log.jsonl");

function lastUser(messages) {
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m.role !== "user") continue;
    if (typeof m.content === "string") return { text: m.content, toolResult: false };
    const toolResult = m.content.some((b) => b.type === "tool_result");
    const text = m.content
      .filter((b) => b.type === "text")
      .map((b) => b.text)
      .join("\n");
    return { text, toolResult };
  }
  return { text: "", toolResult: false };
}

// Every user message since the last assistant turn, since reminders follow the prompt.
function sinceAssistant(messages) {
  const out = [];
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m.role === "assistant") break;
    if (typeof m.content === "string") out.push(m.content);
    else for (const b of m.content) if (b.type === "text") out.push(b.text);
  }
  return out.join("\n");
}

function plan(body) {
  const messages = body.messages || [];
  const last = lastUser(messages);
  const recent = sinceAssistant(messages);
  const system = JSON.stringify(body.system || "");
  if (/summary of the conversation|create a detailed summary/i.test(recent + system)) {
    return { text: "<summary>A fake chat: one reply and one written file.</summary>" };
  }
  if (last.toolResult) return { text: "Wrote fake-out.txt." };
  if (/write a file/i.test(recent)) {
    return {
      tool: {
        name: "Write",
        input: { file_path: path.join(WORK, "fake-out.txt"), content: "fake content\n" },
      },
    };
  }
  if (/say hello/i.test(recent)) return { text: "Hello from the fake API." };
  return { text: "OK." };
}

function sse(res, event, data) {
  res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
}

let n = 0;
const server = http.createServer((req, res) => {
  let raw = "";
  req.on("data", (c) => (raw += c));
  req.on("end", () => {
    const url = req.url || "";
    if (!url.startsWith("/v1/messages")) {
      fs.appendFileSync(LOG, JSON.stringify({ url, method: req.method, status: 404 }) + "\n");
      res.writeHead(404, { "content-type": "application/json" });
      res.end(JSON.stringify({ type: "error", error: { type: "not_found_error", message: "fake" } }));
      return;
    }
    let body = {};
    try {
      body = JSON.parse(raw);
    } catch {}
    if (url.startsWith("/v1/messages/count_tokens")) {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ input_tokens: 1000 }));
      return;
    }
    const step = plan(body);
    fs.appendFileSync(
      LOG,
      JSON.stringify({ url, model: body.model, stream: body.stream, step: step.tool ? "tool" : step.text.slice(0, 40) }) + "\n",
    );
    const id = `msg_fake_${++n}`;
    // Large cached input, so the chat's context reads as big.
    const usage = {
      input_tokens: 1200,
      cache_creation_input_tokens: 0,
      cache_read_input_tokens: Number(process.env.FAKE_CACHED || 240000),
      output_tokens: 20,
    };
    const content = step.tool
      ? [{ type: "tool_use", id: `toolu_fake_${n}`, name: step.tool.name, input: step.tool.input }]
      : [{ type: "text", text: step.text }];
    const stop = step.tool ? "tool_use" : "end_turn";
    if (!body.stream) {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ id, type: "message", role: "assistant", model: body.model, content, stop_reason: stop, stop_sequence: null, usage }));
      return;
    }
    res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
    sse(res, "message_start", {
      type: "message_start",
      message: { id, type: "message", role: "assistant", model: body.model, content: [], stop_reason: null, stop_sequence: null, usage: { ...usage, output_tokens: 1 } },
    });
    content.forEach((block, index) => {
      if (block.type === "text") {
        sse(res, "content_block_start", { type: "content_block_start", index, content_block: { type: "text", text: "" } });
        sse(res, "content_block_delta", { type: "content_block_delta", index, delta: { type: "text_delta", text: block.text } });
      } else {
        sse(res, "content_block_start", { type: "content_block_start", index, content_block: { type: "tool_use", id: block.id, name: block.name, input: {} } });
        sse(res, "content_block_delta", { type: "content_block_delta", index, delta: { type: "input_json_delta", partial_json: JSON.stringify(block.input) } });
      }
      sse(res, "content_block_stop", { type: "content_block_stop", index });
    });
    sse(res, "message_delta", { type: "message_delta", delta: { stop_reason: stop, stop_sequence: null }, usage: { output_tokens: usage.output_tokens } });
    sse(res, "message_stop", { type: "message_stop" });
    res.end();
  });
});
server.listen(PORT, "127.0.0.1", () => console.log(`fake api on ${PORT}`));
