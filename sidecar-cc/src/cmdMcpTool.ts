import { tool, createSdkMcpServer } from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";

// Define a tool: name, description, input schema, handler
const cmdExec = tool(
  "exec",
  "Execute a command",
  {
    addr: z.string().describe("address of the server to send the command to"),
    ssid: z.string().describe("session id"),
    token: z.string().describe("token"),
    cmd: z.string().describe("The command to execute"),
    wait_ms: z.number().describe("Time to wait before executing the command, in milliseconds").optional()
  },
  async (args) => {
    // args is typed from the schema: { latitude: number; longitude: number }
    let url = `http://${args.addr}/api/ssh/send/${args.ssid}`;
    if (args.wait_ms) {
      url += '?wait_ms='+args.wait_ms;
    }
    console.log('sending command to url=============', url);

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'text/plain',
        'Authorization': `Bearer ${args.token}`
      },
      body: args.cmd+'\n'
    });

    const data: any = await response.text();

    // Return a content array - Claude sees this as the tool result
    return {
      content: [{ type: "text", text: `result: ${data}` }]
    };
  }
);

// Wrap the tool in an in-process MCP server
const cmdExecServer = createSdkMcpServer({
  name: "cmd",
  version: "1.0.0",
  tools: [cmdExec]
});

export default cmdExecServer;