export function stripAnsi(text: string): string {
  return text
    .replace(/\u001B\][^\u0007]*(?:\u0007|\u001B\\)/g, "")
    .replace(/\u001B\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\u001B[@-_]/g, "");
}

function isPrompt(line: string): boolean {
  return /[$#]\s*$/.test(line) || /[$#]\s+\S+/.test(line);
}

/** Last command output from a raw terminal stream. */
export function extractLastOutput(text: string): string {
  const markers = ["\u001b]133;C\u0007", "\u001b]633;C\u0007"];
  let at = -1;
  let used = "";
  for (const marker of markers) {
    const found = text.lastIndexOf(marker);
    if (found > at) {
      at = found;
      used = marker;
    }
  }
  if (at !== -1) {
    const rest = text.slice(at + used.length);
    const end = rest.search(/\u001b\](?:133|633);[AD]/);
    const chunk = end === -1 ? rest : rest.slice(0, end);
    return stripAnsi(chunk).replace(/^\r?\n/, "").trimEnd();
  }
  const lines = stripAnsi(text).split("\n");
  const prompts: number[] = [];
  lines.forEach((line, index) => {
    if (isPrompt(line)) prompts.push(index);
  });
  if (prompts.length > 0) {
    const last = prompts[prompts.length - 1];
    const prev = prompts.length > 1 ? prompts[prompts.length - 2] : -1;
    if (last === lines.length - 1 && /[$#]\s*$/.test(lines[last]) && prev !== -1) {
      return lines.slice(prev + 1, last).join("\n").trim();
    }
    return lines.slice(last + 1).join("\n").trim();
  }
  const plain = lines.join("\n");
  const enter = plain.lastIndexOf("\r");
  if (enter !== -1) return plain.slice(enter + 1).replace(/^\n/, "").trimEnd();
  return plain.trimEnd();
}
