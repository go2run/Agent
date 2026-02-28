/**
 * Web Worker — Wasmer-JS WASIX Runtime Bridge
 *
 * Loads @wasmer/sdk from local vendor/ directory (avoiding COEP/CORS issues)
 * and executes WASIX bash in a real WebAssembly sandbox.
 *
 * Protocol:
 *   Main thread → Worker: WorkerCommand (JSON via postMessage)
 *   Worker → Main thread: WorkerEvent (JSON via postMessage)
 *
 * Requirements:
 *   - Page must be cross-origin isolated (COOP + COEP headers) for SharedArrayBuffer
 *   - Worker must be created with { type: "module" }
 *   - vendor/wasmer-sdk/ must be served alongside this file
 */

// ─── State ──────────────────────────────────────────────────

let sdk = null;
let bashPackage = null;
let initPromise = null;   // resolved when SDK is ready (or failed)
let initDone = false;
let runningProcesses = new Map();

// ─── Helpers ────────────────────────────────────────────────

function sendEvent(event) {
    self.postMessage(event);
}

// ─── Initialization ─────────────────────────────────────────

/**
 * Initialize the @wasmer/sdk and pre-load bash.
 * Returns a promise that resolves when ready (or falls back).
 */
function ensureInit() {
    if (initPromise) return initPromise;
    initPromise = doInit();
    return initPromise;
}

async function doInit() {
    try {
        // SharedArrayBuffer check
        if (typeof SharedArrayBuffer === 'undefined') {
            console.warn('[Worker] SharedArrayBuffer not available — fallback mode');
            initDone = true;
            sendEvent({ type: 'Ready' });
            return;
        }

        // Import SDK from local vendor (same-origin, no COEP issues)
        try {
            sdk = await import('./vendor/wasmer-sdk/index.mjs');
            console.log('[Worker] Loaded @wasmer/sdk from local vendor/');
        } catch (e) {
            console.warn('[Worker] Local SDK import failed:', e.message);
            // Try CDN as last resort
            try {
                sdk = await import('https://cdn.jsdelivr.net/npm/@wasmer/sdk@0.10.0/dist/index.mjs');
                console.log('[Worker] Loaded @wasmer/sdk from CDN');
            } catch (e2) {
                console.warn('[Worker] CDN SDK import failed:', e2.message);
            }
        }

        if (!sdk || !sdk.init) {
            console.warn('[Worker] @wasmer/sdk not available — fallback mode');
            initDone = true;
            sendEvent({ type: 'Ready' });
            return;
        }

        // Initialize the Wasmer WASM runtime
        await sdk.init();
        console.log('[Worker] Wasmer SDK runtime initialized');

        // Pre-load bash package from Wasmer registry
        try {
            bashPackage = await sdk.Wasmer.fromRegistry('sharrattj/bash');
            console.log('[Worker] Bash package loaded from Wasmer registry');
        } catch (e) {
            console.warn('[Worker] Bash pre-load failed:', e.message);
        }

        initDone = true;
        sendEvent({ type: 'Ready' });
    } catch (error) {
        console.error('[Worker] Init failed:', error);
        initDone = true;
        sendEvent({ type: 'Ready' });
    }
}

// ─── Command queue ──────────────────────────────────────────
// All commands wait for init to complete before executing.

const commandQueue = [];
let processing = false;

async function enqueue(fn) {
    commandQueue.push(fn);
    if (!processing) drainQueue();
}

async function drainQueue() {
    processing = true;
    // Wait for init before processing any commands
    await ensureInit();
    while (commandQueue.length > 0) {
        const fn = commandQueue.shift();
        try {
            await fn();
        } catch (e) {
            console.error('[Worker] Command error:', e);
        }
    }
    processing = false;
}

// ─── Exec: WASIX bash ──────────────────────────────────────

async function execBash(id, cmd, timeoutMs) {
    try {
        if (sdk && bashPackage) {
            await execBashWasmer(id, cmd, timeoutMs);
        } else {
            await execBashFallback(id, cmd);
        }
    } catch (error) {
        sendEvent({
            type: 'Error',
            id: id,
            message: `Execution error: ${error.message}`,
        });
    }
}

async function execBashWasmer(id, cmd, timeoutMs) {
    let instance = null;
    try {
        // Lazy-load bash if not pre-loaded
        if (!bashPackage) {
            bashPackage = await sdk.Wasmer.fromRegistry('sharrattj/bash');
        }

        instance = await bashPackage.entrypoint.run({
            args: ['-c', cmd],
        });

        runningProcesses.set(id, instance);

        // Timeout
        let timeoutHandle = null;
        let timedOut = false;
        if (timeoutMs) {
            timeoutHandle = setTimeout(() => {
                timedOut = true;
                try { instance?.kill?.(); } catch (_) {}
                sendEvent({ type: 'Error', id, message: `Timeout after ${timeoutMs}ms` });
            }, timeoutMs);
        }

        // Try streaming output, fall back to buffered
        if (instance.stdout && typeof instance.stdout.pipeTo === 'function') {
            const stdoutDone = pipeStream(id, instance.stdout, 'Stdout');
            const stderrDone = instance.stderr
                ? pipeStream(id, instance.stderr, 'Stderr')
                : Promise.resolve();

            const output = await instance.wait();
            await Promise.allSettled([stdoutDone, stderrDone]);

            if (timeoutHandle) clearTimeout(timeoutHandle);
            if (timedOut) return;
            sendEvent({ type: 'ExitCode', id, code: output.code ?? 0 });
        } else {
            const output = await instance.wait();
            if (timeoutHandle) clearTimeout(timeoutHandle);
            if (timedOut) return;

            const dec = new TextDecoder();
            const stdout = output.stdout
                ? (typeof output.stdout === 'string' ? output.stdout : dec.decode(output.stdout))
                : '';
            const stderr = output.stderr
                ? (typeof output.stderr === 'string' ? output.stderr : dec.decode(output.stderr))
                : '';

            if (stdout) sendEvent({ type: 'Stdout', id, data: stdout });
            if (stderr) sendEvent({ type: 'Stderr', id, data: stderr });
            sendEvent({ type: 'ExitCode', id, code: output.code ?? 0 });
        }
    } catch (error) {
        sendEvent({ type: 'Error', id, message: `WASIX error: ${error.message}` });
    } finally {
        runningProcesses.delete(id);
    }
}

async function pipeStream(id, stream, eventType) {
    const dec = new TextDecoder();
    try {
        await stream.pipeTo(new WritableStream({
            write(chunk) {
                const text = typeof chunk === 'string' ? chunk : dec.decode(chunk, { stream: true });
                if (text) sendEvent({ type: eventType, id, data: text });
            },
        }));
    } catch (_) { /* stream cancelled on kill — OK */ }
}

// ─── Exec: Fallback shell ──────────────────────────────────

async function execBashFallback(id, cmd) {
    const parts = cmd.trim().split(/\s+/);
    const command = parts[0];
    const args = parts.slice(1);

    let stdout = '';
    let stderr = '';
    let exitCode = 0;

    switch (command) {
        case 'echo':
            if (args[0] === '-n') {
                stdout = args.slice(1).join(' ');
            } else {
                stdout = args.join(' ') + '\n';
            }
            break;

        case 'printf':
            stdout = args.join(' ').replace(/\\n/g, '\n').replace(/\\t/g, '\t');
            break;

        case 'date':
            stdout = (args.includes('-u') ? new Date().toUTCString() : new Date().toString()) + '\n';
            break;

        case 'whoami':  stdout = 'wasm-agent\n'; break;
        case 'hostname': stdout = 'wasm-agent-host\n'; break;
        case 'pwd':     stdout = '/workspace\n'; break;

        case 'uname':
            stdout = args.includes('-a')
                ? 'WASIX wasm32 wasm32 WASIX wasm32 Wasmer-JS WASIX\n'
                : 'WASIX\n';
            break;

        case 'ls':
            stdout = 'home/\ntmp/\nsrc/\nREADME.md\n';
            break;

        case 'cat':
            if (args[0] === '/etc/os-release') {
                stdout = 'NAME="WASIX"\nVERSION="1.0"\nID=wasix\n';
            } else {
                stderr = `cat: ${args[0] || ''}: No such file or directory\n`;
                exitCode = 1;
            }
            break;

        case 'env':
            stdout = 'SHELL=/bin/bash\nHOME=/workspace/home\nUSER=wasm-agent\nPATH=/bin:/usr/bin\nPWD=/workspace\n';
            break;

        case 'which':
            if (['echo','date','whoami','uname','pwd','ls','cat','env','true','false'].includes(args[0])) {
                stdout = `/usr/bin/${args[0]}\n`;
            } else {
                stderr = `which: no ${args[0]} in (/bin:/usr/bin)\n`;
                exitCode = 1;
            }
            break;

        case 'true':  exitCode = 0; break;
        case 'false': exitCode = 1; break;

        case 'sleep':
            await new Promise(r => setTimeout(r, (parseFloat(args[0]) || 1) * 1000));
            break;

        default:
            stderr = `[Fallback Shell] '${command}': command not found\n`;
            exitCode = 127;
            break;
    }

    if (stdout) sendEvent({ type: 'Stdout', id, data: stdout });
    if (stderr) sendEvent({ type: 'Stderr', id, data: stderr });
    sendEvent({ type: 'ExitCode', id, code: exitCode });
}

// ─── Process management ────────────────────────────────────

function cancelExec(id) {
    const instance = runningProcesses.get(id);
    if (instance) {
        try { instance.kill?.(); } catch (_) {}
        runningProcesses.delete(id);
        sendEvent({ type: 'ExitCode', id, code: 137 });
    }
}

async function writeStdin(id, data) {
    const instance = runningProcesses.get(id);
    if (instance?.stdin) {
        try {
            const writer = instance.stdin.getWriter();
            await writer.write(new TextEncoder().encode(data));
            writer.releaseLock();
        } catch (e) {
            console.warn(`[Worker] stdin write failed for ${id}:`, e.message);
        }
    }
}

// ─── Message handler ───────────────────────────────────────
// All commands go through the queue, which waits for init first.

self.onmessage = function(event) {
    const msg = event.data;

    switch (msg.type) {
        case 'Init':
            // Trigger init (idempotent); commands will wait for it
            ensureInit();
            break;

        case 'ExecBash':
            enqueue(() => execBash(msg.id, msg.cmd, msg.timeout_ms || null));
            break;

        case 'CancelExec':
            cancelExec(msg.id);
            break;

        case 'WriteStdin':
            enqueue(() => writeStdin(msg.id, msg.data));
            break;

        default:
            console.warn('[Worker] Unknown command:', msg.type);
    }
};

// Start init eagerly when the worker loads
ensureInit();

console.log('[Worker] Shell worker loaded (ES module + @wasmer/sdk)');
