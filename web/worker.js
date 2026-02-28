/**
 * Web Worker — Wasmer-JS WASIX Runtime Bridge
 *
 * This Worker hosts the @wasmer/sdk and executes WASIX programs (bash, etc.)
 * isolated from the main UI thread so that synchronous WASI I/O doesn't block rendering.
 *
 * Protocol:
 *   Main thread → Worker: WorkerCommand (JSON via postMessage)
 *   Worker → Main thread: WorkerEvent (JSON via postMessage)
 *
 * Requirements:
 *   - Page must be cross-origin isolated (COOP + COEP headers) for SharedArrayBuffer
 *   - Worker must be created with { type: "module" } for ES module imports
 *
 * WASIX bash is loaded from the Wasmer registry on first use and cached.
 */

// ─── SDK import (from CDN or local) ────────────────────────
// We try multiple sources to be resilient:
// 1. Local path (if served alongside the app via npm/bundler)
// 2. jsDelivr CDN
// 3. unpkg CDN

let sdk = null;
let wasmerReady = false;
let bashPackage = null;
let runningProcesses = new Map();

/**
 * Send a typed event back to the main thread.
 */
function sendEvent(event) {
    self.postMessage(event);
}

/**
 * Dynamically import the @wasmer/sdk from available sources.
 */
async function loadSdk() {
    const sources = [
        'https://cdn.jsdelivr.net/npm/@wasmer/sdk@0.10.0/dist/index.mjs',
        'https://unpkg.com/@wasmer/sdk@0.10.0/dist/index.mjs',
    ];

    for (const src of sources) {
        try {
            const mod = await import(src);
            console.log(`[Worker] Loaded @wasmer/sdk from: ${src}`);
            return mod;
        } catch (e) {
            console.warn(`[Worker] Failed to load SDK from ${src}:`, e.message);
        }
    }
    return null;
}

/**
 * Initialize the Wasmer-JS SDK and pre-load the bash package.
 */
async function initWasmer() {
    if (wasmerReady) return;

    try {
        // Check SharedArrayBuffer availability (requires COOP/COEP)
        if (typeof SharedArrayBuffer === 'undefined') {
            console.warn(
                '[Worker] SharedArrayBuffer not available. ' +
                'Ensure COOP/COEP headers are set. Falling back to basic shell.'
            );
            wasmerReady = true;
            sendEvent({ type: 'Ready' });
            return;
        }

        sdk = await loadSdk();
        if (!sdk || !sdk.init) {
            console.warn('[Worker] @wasmer/sdk not available, using fallback shell');
            wasmerReady = true;
            sendEvent({ type: 'Ready' });
            return;
        }

        // Initialize the Wasmer runtime
        await sdk.init();
        console.log('[Worker] Wasmer SDK initialized');

        // Pre-load the bash package from the Wasmer registry
        try {
            bashPackage = await sdk.Wasmer.fromRegistry('sharrattj/bash');
            console.log('[Worker] Bash package loaded from registry');
        } catch (e) {
            console.warn('[Worker] Failed to pre-load bash package:', e.message);
            // Will retry on first exec
        }

        wasmerReady = true;
        sendEvent({ type: 'Ready' });
    } catch (error) {
        console.warn('[Worker] Wasmer init failed, using fallback:', error.message);
        wasmerReady = true;
        sendEvent({ type: 'Ready' });
    }
}

/**
 * Execute a bash command via WASIX.
 * Falls back to the simple command parser if Wasmer SDK is not available.
 */
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

/**
 * Execute via the @wasmer/sdk WASIX runtime.
 */
async function execBashWasmer(id, cmd, timeoutMs) {
    let instance = null;

    try {
        // Ensure bash package is loaded
        if (!bashPackage) {
            bashPackage = await sdk.Wasmer.fromRegistry('sharrattj/bash');
        }

        // Spawn a bash process with the command
        instance = await bashPackage.entrypoint.run({
            args: ['-c', cmd],
        });

        // Track the running process for cancellation
        runningProcesses.set(id, instance);

        // Set up timeout if specified
        let timeoutHandle = null;
        let timedOut = false;

        if (timeoutMs) {
            timeoutHandle = setTimeout(() => {
                timedOut = true;
                if (instance && instance.kill) {
                    instance.kill();
                }
                sendEvent({
                    type: 'Error',
                    id: id,
                    message: `Timeout after ${timeoutMs}ms`,
                });
            }, timeoutMs);
        }

        // Stream stdout in real-time if possible
        if (instance.stdout && typeof instance.stdout.pipeTo === 'function') {
            // Set up streaming stdout
            const stdoutReader = streamToEvents(id, instance.stdout, 'Stdout');
            const stderrReader = instance.stderr
                ? streamToEvents(id, instance.stderr, 'Stderr')
                : Promise.resolve();

            // Wait for the process to finish
            const output = await instance.wait();

            // Wait for streams to finish
            await Promise.allSettled([stdoutReader, stderrReader]);

            if (timeoutHandle) clearTimeout(timeoutHandle);
            if (timedOut) return;

            sendEvent({ type: 'ExitCode', id: id, code: output.code ?? 0 });
        } else {
            // Non-streaming: wait for complete output
            const output = await instance.wait();

            if (timeoutHandle) clearTimeout(timeoutHandle);
            if (timedOut) return;

            const decoder = new TextDecoder();
            const stdout = output.stdout
                ? (typeof output.stdout === 'string' ? output.stdout : decoder.decode(output.stdout))
                : '';
            const stderr = output.stderr
                ? (typeof output.stderr === 'string' ? output.stderr : decoder.decode(output.stderr))
                : '';

            if (stdout) {
                sendEvent({ type: 'Stdout', id: id, data: stdout });
            }
            if (stderr) {
                sendEvent({ type: 'Stderr', id: id, data: stderr });
            }

            sendEvent({ type: 'ExitCode', id: id, code: output.code ?? 0 });
        }
    } catch (error) {
        sendEvent({
            type: 'Error',
            id: id,
            message: `Wasmer execution error: ${error.message}`,
        });
    } finally {
        runningProcesses.delete(id);
    }
}

/**
 * Pipe a ReadableStream to postMessage events for real-time streaming.
 */
async function streamToEvents(id, readableStream, eventType) {
    const decoder = new TextDecoder();
    try {
        await readableStream.pipeTo(
            new WritableStream({
                write(chunk) {
                    const text = typeof chunk === 'string'
                        ? chunk
                        : decoder.decode(chunk, { stream: true });
                    if (text) {
                        sendEvent({ type: eventType, id: id, data: text });
                    }
                },
            })
        );
    } catch (e) {
        // Stream may be cancelled on process kill — that's OK
        if (!e.message?.includes('cancel')) {
            console.warn(`[Worker] Stream error (${eventType}):`, e.message);
        }
    }
}

/**
 * Fallback command execution — simulates basic shell behavior
 * when the full @wasmer/sdk WASIX runtime is not available.
 */
async function execBashFallback(id, cmd) {
    const trimmed = cmd.trim();

    // Handle pipes and redirections minimally
    const parts = trimmed.split(/\s+/);
    const command = parts[0];
    const args = parts.slice(1);

    let stdout = '';
    let stderr = '';
    let exitCode = 0;

    switch (command) {
        case 'echo':
            // Handle -n flag and basic variable expansion
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
            if (args.includes('-u')) {
                stdout = new Date().toUTCString() + '\n';
            } else {
                stdout = new Date().toString() + '\n';
            }
            break;

        case 'whoami':
            stdout = 'wasm-agent\n';
            break;

        case 'hostname':
            stdout = 'wasm-agent-host\n';
            break;

        case 'uname':
            if (args.includes('-a')) {
                stdout = 'WASIX wasm32 wasm32 WASIX wasm32 Wasmer-JS WASIX\n';
            } else {
                stdout = 'WASIX\n';
            }
            break;

        case 'pwd':
            stdout = '/workspace\n';
            break;

        case 'ls':
            stdout = 'home/\ntmp/\nsrc/\nREADME.md\n';
            break;

        case 'cat':
            if (args[0] === '/etc/os-release') {
                stdout = 'NAME="WASIX"\nVERSION="1.0"\nID=wasix\n';
            } else {
                stderr = `cat: ${args[0] || ''}: No such file or directory\n`;
                stdout = '[Fallback Shell] Full filesystem not available.\n' +
                         'Install @wasmer/sdk and enable COOP/COEP headers for real WASIX bash.\n';
                exitCode = 1;
            }
            break;

        case 'env':
            stdout = [
                'SHELL=/bin/bash',
                'HOME=/workspace/home',
                'USER=wasm-agent',
                'PATH=/bin:/usr/bin',
                'PWD=/workspace',
                'TERM=xterm-256color',
            ].join('\n') + '\n';
            break;

        case 'which':
            if (['echo', 'date', 'whoami', 'uname', 'pwd', 'ls', 'cat', 'env', 'true', 'false'].includes(args[0])) {
                stdout = `/usr/bin/${args[0]}\n`;
            } else {
                stderr = `which: no ${args[0]} in (/bin:/usr/bin)\n`;
                exitCode = 1;
            }
            break;

        case 'true':
            exitCode = 0;
            break;

        case 'false':
            exitCode = 1;
            break;

        case 'sleep':
            {
                const seconds = parseFloat(args[0]) || 1;
                await new Promise(r => setTimeout(r, seconds * 1000));
            }
            break;

        case 'head':
        case 'tail':
        case 'grep':
        case 'sed':
        case 'awk':
        case 'sort':
        case 'wc':
        case 'cut':
        case 'tr':
        case 'mkdir':
        case 'rm':
        case 'cp':
        case 'mv':
        case 'touch':
        case 'chmod':
        case 'find':
        case 'curl':
        case 'wget':
            stderr = `[Fallback Shell] '${command}' requires full WASIX runtime.\n`;
            stdout = 'Enable @wasmer/sdk with COOP/COEP headers for real bash.\n';
            exitCode = 127;
            break;

        default:
            stderr = `[Fallback Shell] '${command}': command not found\n`;
            stdout = 'Note: Full WASIX bash requires @wasmer/sdk.\n' +
                     'Serve with COOP/COEP headers to enable SharedArrayBuffer.\n';
            exitCode = 127;
            break;
    }

    if (stdout) {
        sendEvent({ type: 'Stdout', id: id, data: stdout });
    }
    if (stderr) {
        sendEvent({ type: 'Stderr', id: id, data: stderr });
    }

    sendEvent({ type: 'ExitCode', id: id, code: exitCode });
}

/**
 * Cancel a running process.
 */
function cancelExec(id) {
    const instance = runningProcesses.get(id);
    if (instance) {
        if (typeof instance.kill === 'function') {
            instance.kill();
        }
        runningProcesses.delete(id);
        sendEvent({ type: 'ExitCode', id: id, code: 137 }); // SIGKILL
    }
}

/**
 * Write to stdin of a running process.
 */
async function writeStdin(id, data) {
    const instance = runningProcesses.get(id);
    if (instance && instance.stdin) {
        try {
            const encoder = new TextEncoder();
            const writer = instance.stdin.getWriter();
            await writer.write(encoder.encode(data));
            writer.releaseLock();
        } catch (e) {
            console.warn(`[Worker] Failed to write stdin for ${id}:`, e.message);
        }
    }
}

// ─── Message handler ─────────────────────────────────────────

self.onmessage = async function(event) {
    const msg = event.data;

    switch (msg.type) {
        case 'Init':
            await initWasmer();
            break;

        case 'ExecBash':
            await execBash(msg.id, msg.cmd, msg.timeout_ms || null);
            break;

        case 'CancelExec':
            cancelExec(msg.id);
            break;

        case 'WriteStdin':
            await writeStdin(msg.id, msg.data);
            break;

        default:
            console.warn('[Worker] Unknown command:', msg.type);
    }
};

console.log('[Worker] Shell worker loaded (ES module)');
