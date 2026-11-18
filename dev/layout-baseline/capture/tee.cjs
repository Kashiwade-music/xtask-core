'use strict';
// Stand-in for XTASK_NODE: runs the real runner and records each request/response line pair.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');

const outDir = process.env.XTASK_CAPTURE_DIR;
fs.mkdirSync(outDir, { recursive: true });
const log = fs.createWriteStream(path.join(outDir, `session-${process.pid}-${Date.now()}.jsonl`));
const child = spawn(process.execPath, process.argv.slice(2), { stdio: ['pipe', 'pipe', 'pipe'] });
child.stderr.pipe(process.stderr);
const pending = [];
// xtask kills the cmd.exe wrapper (our parent) without closing stdin first and then
// waits for stderr EOF, so exit as soon as the parent disappears.
const parent = process.ppid;
setInterval(() => {
    try { process.kill(parent, 0); } catch { child.kill(); log.end(() => process.exit(0)); }
}, 200).unref();

readline.createInterface({ input: process.stdin, crlfDelay: Infinity }).on('line', line => {
    pending.push(line);
    child.stdin.write(line + '\n');
}).on('close', () => child.stdin.end());

readline.createInterface({ input: child.stdout, crlfDelay: Infinity }).on('line', line => {
    const request = pending.shift();
    log.write(JSON.stringify({ request: JSON.parse(request), response: JSON.parse(line) }) + '\n');
    process.stdout.write(line + '\n');
});

child.on('exit', code => log.end(() => process.exit(code ?? 1)));
