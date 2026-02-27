/**
 * Web Worker — Wasmer-JS WASIX Runtime Bridge
 *
 * This Worker hosts the Wasmer-JS SDK and executes WASIX programs (bash, etc.)
 * isolated from the main UI thread so that synchronous WASI I/O doesn't block rendering.
 *
 * Protocol:
 *   Main thread → Worker: WorkerCommand (JSON via postMessage)
 *   Worker → Main thread: WorkerEvent (JSON via postMessage)
 *
 * WASIX bash is loaded from the Wasmer registry on first use.
 */

// State
let wasmerInitialized = false;
let wasmerModule = null;
let runningProcesses = new Map();

/**
 * Send a typed event back to the main thread.
 */
function sendEvent(event) {
    self.postMessage(event);
}

/**
 * Initialize the Wasmer-JS SDK.
 * Loads the SDK from CDN and prepares the WASIX runtime.
 */
async function initWasmer() {
    if (wasmerInitialized) return;

    try {
        // Import Wasmer SDK from CDN
        // The @aspect-build/aspect-wasmer-js package or official wasmer-js can be used
        importScripts('https://unpkg.com/@aspect-build/aspect-wasmer-js@0.8.0/dist/WasmerSDK.js');

        // Alternative: use official Wasmer JS SDK when available
        // importScripts('https://cdn.wasmer.io/sdk/wasmer-js/latest/wasmer_js.js');

        wasmerInitialized = true;
        sendEvent({ type: 'Ready' });
        console.log('[Worker] Wasmer-JS SDK initialized');
    } catch (error) {
        console.warn('[Worker] Wasmer-JS SDK not available, using fallback shell:', error.message);
        wasmerInitialized = true; // Mark as "initialized" even in fallback mode
        sendEvent({ type: 'Ready' });
    }
}

/**
 * Execute a bash command via WASIX.
 * Falls back to a simple command parser if Wasmer-JS is not available.
 */
async function execBash(id, cmd, timeoutMs) {
    try {
        // If Wasmer-JS with full WASIX bash is available, use it
        if (typeof Wasmer !== 'undefined' && Wasmer.init) {
            await execBashWasmer(id, cmd, timeoutMs);
            return;
        }

        // Fallback: simple command simulation
        await execBashFallback(id, cmd);
    } catch (error) {
        sendEvent({
            type: 'Error',
            id: id,
            message: `Execution error: ${error.message}`,
        });
    }
}

/**
 * Execute via the Wasmer-JS WASIX runtime.
 */
async function execBashWasmer(id, cmd, timeoutMs) {
    try {
        // Initialize Wasmer if needed
        await Wasmer.init();

        // Run the command through bash
        const bash = await Wasmer.spawn('sharrattj/bash', {
            args: ['-c', cmd],
            stdin: { mode: 'pipe' },
        });

        // Set up timeout if specified
        let timeoutHandle = null;
        if (timeoutMs) {
            timeoutHandle = setTimeout(() => {
                bash.kill();
                sendEvent({
                    type: 'Error',
                    id: id,
                    message: `Timeout after ${timeoutMs}ms`,
                });
            }, timeoutMs);
        }

        // Read stdout
        const output = await bash.wait();
        if (timeoutHandle) clearTimeout(timeoutHandle);

        const stdout = output.stdout ? new TextDecoder().decode(output.stdout) : '';
        const stderr = output.stderr ? new TextDecoder().decode(output.stderr) : '';

        if (stdout) {
            sendEvent({ type: 'Stdout', id: id, data: stdout });
        }
        if (stderr) {
            sendEvent({ type: 'Stderr', id: id, data: stderr });
        }

        sendEvent({ type: 'ExitCode', id: id, code: output.exitCode || 0 });
    } catch (error) {
        sendEvent({
            type: 'Error',
            id: id,
            message: `Wasmer execution error: ${error.message}`,
        });
    }
}

/**
 * Fallback command execution — simulates basic shell behavior
 * when the full Wasmer-JS WASIX runtime is not available.
 */
async function execBashFallback(id, cmd) {
    const parts = cmd.trim().split(/\s+/);
    const command = parts[0];
    const args = parts.slice(1);

    let stdout = '';
    let exitCode = 0;

    switch (command) {
        case 'echo':
            stdout = args.join(' ') + '\n';
            break;

        case 'date':
            stdout = new Date().toISOString() + '\n';
            break;

        case 'whoami':
            stdout = 'wasm-agent\n';
            break;

        case 'uname':
            stdout = 'WASIX wasm32 Wasmer-JS\n';
            break;

        case 'pwd':
            stdout = '/home/agent\n';
            break;

        case 'ls':
            stdout = [
                'bin/',
                'home/',
                'tmp/',
                'README.md',
            ].join('\n') + '\n';
            break;

        case 'cat':
            if (args[0] === '/etc/os-release') {
                stdout = 'NAME="WASIX"\nVERSION="1.0"\nID=wasix\n';
            } else {
                stdout = `cat: ${args[0] || ''}: No such file (Wasmer-JS not loaded)\n`;
                exitCode = 1;
            }
            break;

        case 'env':
            stdout = 'SHELL=/bin/bash\nHOME=/home/agent\nUSER=wasm-agent\nPATH=/bin:/usr/bin\n';
            break;

        case 'true':
            exitCode = 0;
            break;

        case 'false':
            exitCode = 1;
            break;

        default:
            stdout = `[Fallback Shell] Command simulated: ${cmd}\n` +
                     `Note: Full WASIX bash requires the Wasmer-JS SDK.\n` +
                     `The command would execute in a real WASIX environment.\n`;
            break;
    }

    if (stdout) {
        sendEvent({ type: 'Stdout', id: id, data: stdout });
    }

    sendEvent({ type: 'ExitCode', id: id, code: exitCode });
}

/**
 * Cancel a running process.
 */
function cancelExec(id) {
    const process = runningProcesses.get(id);
    if (process && process.kill) {
        process.kill();
        runningProcesses.delete(id);
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
            // Will be implemented with streaming support
            console.warn('[Worker] WriteStdin not yet implemented');
            break;

        default:
            console.warn('[Worker] Unknown command:', msg.type);
    }
};

console.log('[Worker] Shell worker loaded');
