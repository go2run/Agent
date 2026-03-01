/**
 * Web Worker — Wasmer-JS WASIX Runtime Bridge
 *
 * Loads @wasmer/sdk and executes WASIX packages in a real WebAssembly sandbox.
 * Supports dynamic package installation from the Wasmer registry.
 *
 * Protocol (JSON via postMessage):
 *   Main thread → Worker: WorkerCommand
 *   Worker → Main thread: WorkerEvent
 *
 * Features:
 *   - Auto-downloads WASIX packages from registry on demand
 *   - Caches downloaded packages for reuse
 *   - Falls back to a built-in JS shell when SDK is unavailable
 *   - Timeout protection with init and per-command timeouts
 *
 * Requirements:
 *   - Page must be cross-origin isolated (COOP + COEP headers) for SharedArrayBuffer
 *   - Worker must be created with { type: "module" }
 */

// ─── State ──────────────────────────────────────────────────

let sdk = null;
let sdkAvailable = false;
let initPromise = null;
let initDone = false;
let runningProcesses = new Map();

/**
 * Package cache: maps registry name → loaded Wasmer.Package
 * e.g. "sharrattj/bash" → Package
 */
const packageCache = new Map();

/**
 * Well-known command → package mapping.
 * When a command is not found in fallback mode, we try to auto-install from registry.
 */
const COMMAND_PACKAGE_MAP = {
    'bash':       'sharrattj/bash',
    'sh':         'sharrattj/bash',
    'python':     'nicolo-ribaudo/python',
    'python3':    'nicolo-ribaudo/python',
    'node':       'nicolo-ribaudo/node',
    'cowsay':     'syrusakbary/cowsay',
    'figlet':     'syrusakbary/figlet',
    'sqlite':     'nicolo-ribaudo/sqlite',
    'sqlite3':    'nicolo-ribaudo/sqlite',
    'coreutils':  'sharrattj/coreutils',
    'ls':         'sharrattj/coreutils',
    'cat':        'sharrattj/coreutils',
    'head':       'sharrattj/coreutils',
    'tail':       'sharrattj/coreutils',
    'sort':       'sharrattj/coreutils',
    'uniq':       'sharrattj/coreutils',
    'wc':         'sharrattj/coreutils',
    'grep':       'sharrattj/coreutils',
    'sed':        'sharrattj/coreutils',
    'awk':        'sharrattj/coreutils',
    'tr':         'sharrattj/coreutils',
    'cut':        'sharrattj/coreutils',
    'tee':        'sharrattj/coreutils',
    'find':       'sharrattj/coreutils',
    'xargs':      'sharrattj/coreutils',
    'curl':       'nicolo-ribaudo/curl',
    'wget':       'nicolo-ribaudo/wget',
    'vim':        'nicolo-ribaudo/vim',
    'nano':       'nicolo-ribaudo/nano',
    'tree':       'nicolo-ribaudo/tree',
    'jq':         'nicolo-ribaudo/jq',
};

// ─── Helpers ────────────────────────────────────────────────

function sendEvent(event) {
    self.postMessage(event);
}

function log(msg) {
    console.log(`[Worker] ${msg}`);
}

function warn(msg) {
    console.warn(`[Worker] ${msg}`);
}

// ─── Initialization ─────────────────────────────────────────

function ensureInit() {
    if (initPromise) return initPromise;
    initPromise = doInit();
    return initPromise;
}

async function doInit() {
    try {
        // SharedArrayBuffer check
        if (typeof SharedArrayBuffer === 'undefined') {
            warn('SharedArrayBuffer not available — fallback mode');
            initDone = true;
            sendEvent({ type: 'Ready' });
            return;
        }

        // Import SDK — try local vendor first, then CDN
        let loadedSdk = null;
        try {
            loadedSdk = await import('./vendor/wasmer-sdk/index.mjs');
            log('Loaded @wasmer/sdk from local vendor/');
        } catch (e) {
            warn(`Local SDK import failed: ${e.message}`);
            try {
                loadedSdk = await import('https://cdn.jsdelivr.net/npm/@wasmer/sdk@0.10.0/dist/index.mjs');
                log('Loaded @wasmer/sdk from CDN');
            } catch (e2) {
                warn(`CDN SDK import failed: ${e2.message}`);
            }
        }

        if (!loadedSdk || !loadedSdk.init) {
            warn('@wasmer/sdk not available — fallback mode');
            initDone = true;
            sendEvent({ type: 'Ready' });
            return;
        }

        // Initialize the Wasmer WASM runtime with a timeout
        try {
            await Promise.race([
                loadedSdk.init(),
                new Promise((_, reject) =>
                    setTimeout(() => reject(new Error('SDK init timeout (15s)')), 15000)
                ),
            ]);
            sdk = loadedSdk;
            sdkAvailable = true;
            log('Wasmer SDK runtime initialized');
        } catch (e) {
            warn(`SDK init failed: ${e.message} — fallback mode`);
            initDone = true;
            sendEvent({ type: 'Ready' });
            return;
        }

        // Pre-load bash package (non-blocking, with timeout)
        try {
            const bashPkg = await Promise.race([
                sdk.Wasmer.fromRegistry('sharrattj/bash'),
                new Promise((_, reject) =>
                    setTimeout(() => reject(new Error('Bash pre-load timeout (30s)')), 30000)
                ),
            ]);
            packageCache.set('sharrattj/bash', bashPkg);
            log('Bash package pre-loaded from Wasmer registry');
        } catch (e) {
            warn(`Bash pre-load failed: ${e.message} (will try on demand)`);
        }

        initDone = true;
        sendEvent({ type: 'Ready' });
    } catch (error) {
        warn(`Init failed: ${error.message}`);
        initDone = true;
        sendEvent({ type: 'Ready' });
    }
}

// ─── Package Management ─────────────────────────────────────

/**
 * Install (or retrieve from cache) a WASIX package from the Wasmer registry.
 * @param {string} packageName - e.g. "sharrattj/bash"
 * @returns {Promise<object|null>} The loaded package, or null on failure
 */
async function installPackage(packageName) {
    // Check cache first
    if (packageCache.has(packageName)) {
        return packageCache.get(packageName);
    }

    if (!sdkAvailable || !sdk) {
        return null;
    }

    try {
        log(`Installing package: ${packageName}`);
        const pkg = await Promise.race([
            sdk.Wasmer.fromRegistry(packageName),
            new Promise((_, reject) =>
                setTimeout(() => reject(new Error(`Install timeout for ${packageName}`)), 60000)
            ),
        ]);
        packageCache.set(packageName, pkg);
        log(`Package installed: ${packageName}`);
        return pkg;
    } catch (e) {
        warn(`Failed to install ${packageName}: ${e.message}`);
        return null;
    }
}

/**
 * Resolve a command name to a registry package name.
 */
function resolveCommandPackage(command) {
    return COMMAND_PACKAGE_MAP[command] || null;
}

// ─── Command Queue ──────────────────────────────────────────

const commandQueue = [];
let processing = false;

async function enqueue(fn) {
    commandQueue.push(fn);
    if (!processing) drainQueue();
}

async function drainQueue() {
    processing = true;
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
        if (sdkAvailable) {
            await execBashWasmer(id, cmd, timeoutMs);
        } else {
            await execBashFallback(id, cmd);
        }
    } catch (error) {
        sendEvent({ type: 'Error', id, message: `Execution error: ${error.message}` });
    }
}

async function execBashWasmer(id, cmd, timeoutMs) {
    let instance = null;
    try {
        // Get bash package (from cache or install)
        let bashPkg = await installPackage('sharrattj/bash');
        if (!bashPkg) {
            // Fall back to JS shell if bash can't be loaded
            await execBashFallback(id, cmd);
            return;
        }

        instance = await bashPkg.entrypoint.run({
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

// ─── Exec: Package entrypoint ──────────────────────────────

async function execPackage(id, packageName, args, timeoutMs) {
    try {
        const pkg = await installPackage(packageName);
        if (!pkg) {
            sendEvent({ type: 'Error', id, message: `Failed to install package: ${packageName}` });
            return;
        }

        if (!pkg.entrypoint) {
            sendEvent({ type: 'Error', id, message: `Package ${packageName} has no entrypoint` });
            return;
        }

        const instance = await pkg.entrypoint.run({ args });
        runningProcesses.set(id, instance);

        let timeoutHandle = null;
        let timedOut = false;
        if (timeoutMs) {
            timeoutHandle = setTimeout(() => {
                timedOut = true;
                try { instance?.kill?.(); } catch (_) {}
                sendEvent({ type: 'Error', id, message: `Timeout after ${timeoutMs}ms` });
            }, timeoutMs);
        }

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
    } catch (error) {
        sendEvent({ type: 'Error', id, message: `Package exec error: ${error.message}` });
    } finally {
        runningProcesses.delete(id);
    }
}

// ─── Install Package Command ───────────────────────────────

async function handleInstallPackage(id, packageName) {
    const cached = packageCache.has(packageName);
    const pkg = await installPackage(packageName);
    if (pkg) {
        sendEvent({ type: 'PackageInstalled', id, package: packageName, cached });
    } else {
        sendEvent({ type: 'Error', id, message: `Failed to install: ${packageName}` });
    }
}

// ─── List Packages Command ─────────────────────────────────

function handleListPackages(id) {
    const packages = Array.from(packageCache.keys());
    sendEvent({ type: 'PackageList', id, packages });
}

// ─── Exec: Fallback shell ──────────────────────────────────
// A minimal JS-based shell for when @wasmer/sdk is not available.
// Handles basic builtins. For unknown commands, tries to auto-install
// from the Wasmer registry if SDK becomes available.

async function execBashFallback(id, cmd) {
    // Simple command parsing (handles pipes at top level)
    const trimmed = cmd.trim();

    // Handle empty command
    if (!trimmed) {
        sendEvent({ type: 'ExitCode', id, code: 0 });
        return;
    }

    // Split by pipe for simple pipeline support
    const pipeSegments = trimmed.split(/\s*\|\s*/);
    if (pipeSegments.length > 1) {
        // For pipelines, run sequentially and pipe stdout
        let input = '';
        for (let i = 0; i < pipeSegments.length; i++) {
            const isLast = i === pipeSegments.length - 1;
            const result = await runSingleCommand(pipeSegments[i], input);
            if (result.exitCode !== 0 && !isLast) {
                if (result.stderr) sendEvent({ type: 'Stderr', id, data: result.stderr });
                sendEvent({ type: 'ExitCode', id, code: result.exitCode });
                return;
            }
            input = result.stdout;
            if (result.stderr) sendEvent({ type: 'Stderr', id, data: result.stderr });
        }
        if (input) sendEvent({ type: 'Stdout', id, data: input });
        sendEvent({ type: 'ExitCode', id, code: 0 });
        return;
    }

    const result = await runSingleCommand(trimmed, '');
    if (result.stdout) sendEvent({ type: 'Stdout', id, data: result.stdout });
    if (result.stderr) sendEvent({ type: 'Stderr', id, data: result.stderr });
    sendEvent({ type: 'ExitCode', id, code: result.exitCode });
}

async function runSingleCommand(cmdStr, stdinData) {
    const parts = cmdStr.trim().split(/\s+/);
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
            stdout = args.join(' ')
                .replace(/\\n/g, '\n')
                .replace(/\\t/g, '\t')
                .replace(/\\r/g, '\r');
            break;

        case 'date':
            stdout = (args.includes('-u') ? new Date().toUTCString() : new Date().toString()) + '\n';
            break;

        case 'whoami':  stdout = 'wasm-agent\n'; break;
        case 'hostname': stdout = 'wasm-agent-host\n'; break;
        case 'pwd':     stdout = '/workspace\n'; break;
        case 'id':      stdout = 'uid=1000(wasm-agent) gid=1000(wasm-agent) groups=1000(wasm-agent)\n'; break;

        case 'uname':
            if (args.includes('-a')) {
                stdout = 'WASIX wasm32 wasm32 WASIX wasm32 Wasmer-JS WASIX\n';
            } else if (args.includes('-s')) {
                stdout = 'WASIX\n';
            } else if (args.includes('-m')) {
                stdout = 'wasm32\n';
            } else {
                stdout = 'WASIX\n';
            }
            break;

        case 'ls':
            stdout = 'home/\ntmp/\nsrc/\nREADME.md\n';
            break;

        case 'cat':
            if (stdinData) {
                stdout = stdinData;
            } else if (args[0] === '/etc/os-release') {
                stdout = 'NAME="WASIX"\nVERSION="1.0"\nID=wasix\n';
            } else if (args.length === 0) {
                stdout = stdinData || '';
            } else {
                stderr = `cat: ${args[0]}: No such file or directory\n`;
                exitCode = 1;
            }
            break;

        case 'env':
            stdout = 'SHELL=/bin/bash\nHOME=/workspace/home\nUSER=wasm-agent\nPATH=/bin:/usr/bin\nPWD=/workspace\nLANG=en_US.UTF-8\n';
            break;

        case 'export':
            stdout = '';
            break;

        case 'cd':
            stdout = '';
            break;

        case 'head':
            if (stdinData) {
                const n = args.includes('-n') ? parseInt(args[args.indexOf('-n') + 1]) || 10 : 10;
                stdout = stdinData.split('\n').slice(0, n).join('\n') + '\n';
            } else {
                stderr = 'head: missing operand\n';
                exitCode = 1;
            }
            break;

        case 'tail':
            if (stdinData) {
                const n = args.includes('-n') ? parseInt(args[args.indexOf('-n') + 1]) || 10 : 10;
                const lines = stdinData.split('\n').filter(Boolean);
                stdout = lines.slice(-n).join('\n') + '\n';
            } else {
                stderr = 'tail: missing operand\n';
                exitCode = 1;
            }
            break;

        case 'wc':
            if (stdinData || args.length === 0) {
                const text = stdinData || '';
                const lines = text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
                const words = text.split(/\s+/).filter(Boolean).length;
                const chars = text.length;
                stdout = `${lines} ${words} ${chars}\n`;
            } else {
                stderr = `wc: ${args[0]}: No such file or directory\n`;
                exitCode = 1;
            }
            break;

        case 'sort':
            if (stdinData) {
                const lines = stdinData.split('\n').filter(Boolean);
                lines.sort();
                if (args.includes('-r')) lines.reverse();
                stdout = lines.join('\n') + '\n';
            }
            break;

        case 'uniq':
            if (stdinData) {
                const lines = stdinData.split('\n');
                const unique = lines.filter((line, i) => i === 0 || line !== lines[i - 1]);
                stdout = unique.join('\n');
            }
            break;

        case 'tr':
            if (stdinData && args.length >= 2) {
                const from = args[0].replace(/'/g, '');
                const to = args[1].replace(/'/g, '');
                let result = stdinData;
                for (let i = 0; i < from.length && i < to.length; i++) {
                    result = result.split(from[i]).join(to[i]);
                }
                stdout = result;
            }
            break;

        case 'rev':
            if (stdinData) {
                stdout = stdinData.split('\n').map(l => l.split('').reverse().join('')).join('\n');
            }
            break;

        case 'which': {
            const allCmds = [
                'echo','date','whoami','uname','pwd','ls','cat','env','true','false',
                'head','tail','wc','sort','uniq','tr','rev','printf','sleep','id',
                'which','type','help','pkg-install','pkg-list',
            ];
            if (allCmds.includes(args[0])) {
                stdout = `/usr/bin/${args[0]}\n`;
            } else if (resolveCommandPackage(args[0])) {
                stdout = `/wasmer/bin/${args[0]} (installable: ${resolveCommandPackage(args[0])})\n`;
            } else {
                stderr = `which: no ${args[0]} in (/bin:/usr/bin)\n`;
                exitCode = 1;
            }
            break;
        }

        case 'type': {
            const builtins = ['echo','printf','cd','export','true','false','type','help'];
            if (builtins.includes(args[0])) {
                stdout = `${args[0]} is a shell builtin\n`;
            } else if (resolveCommandPackage(args[0])) {
                stdout = `${args[0]} is available from Wasmer registry: ${resolveCommandPackage(args[0])}\n`;
            } else {
                stderr = `type: ${args[0]}: not found\n`;
                exitCode = 1;
            }
            break;
        }

        case 'help':
            stdout = [
                'WASM Agent Shell — Built-in commands:',
                '  echo, printf, date, whoami, hostname, pwd, id, uname',
                '  ls, cat, head, tail, wc, sort, uniq, tr, rev',
                '  env, export, cd, which, type, true, false, sleep',
                '',
                'Package management (auto-download from Wasmer registry):',
                '  pkg-install <name>   — Install a WASIX package (e.g. sharrattj/bash)',
                '  pkg-list             — List cached packages',
                '',
                'When SDK is available, any command can be run via WASIX bash.',
                'Unknown commands will try auto-install from the Wasmer registry.',
                '',
            ].join('\n');
            break;

        // ── Package management builtins ──────────────────────
        case 'pkg-install':
            if (!args[0]) {
                stderr = 'Usage: pkg-install <package-name>\n  e.g. pkg-install sharrattj/bash\n';
                exitCode = 1;
            } else {
                const pkg = await installPackage(args[0]);
                if (pkg) {
                    stdout = `Installed: ${args[0]}\n`;
                } else {
                    stderr = `Failed to install: ${args[0]}\n`;
                    exitCode = 1;
                }
            }
            break;

        case 'pkg-list':
            if (packageCache.size === 0) {
                stdout = '(no packages installed)\n';
            } else {
                stdout = Array.from(packageCache.keys()).map(k => `  ${k}`).join('\n') + '\n';
            }
            break;

        case 'true':  exitCode = 0; break;
        case 'false': exitCode = 1; break;

        case 'sleep':
            await new Promise(r => setTimeout(r, (parseFloat(args[0]) || 1) * 1000));
            break;

        case 'seq': {
            const start = args.length >= 2 ? parseInt(args[0]) : 1;
            const end = args.length >= 2 ? parseInt(args[1]) : parseInt(args[0]);
            const lines = [];
            for (let i = start; i <= end; i++) lines.push(String(i));
            stdout = lines.join('\n') + '\n';
            break;
        }

        case 'yes':
            // Limited to 100 lines to avoid infinite loop
            stdout = Array(100).fill(args[0] || 'y').join('\n') + '\n';
            break;

        default: {
            // Try auto-install from Wasmer registry
            const packageName = resolveCommandPackage(command);
            if (packageName && sdkAvailable) {
                const pkg = await installPackage(packageName);
                if (pkg && pkg.entrypoint) {
                    try {
                        const instance = await pkg.entrypoint.run({ args });
                        const output = await instance.wait();
                        const dec = new TextDecoder();
                        stdout = output.stdout
                            ? (typeof output.stdout === 'string' ? output.stdout : dec.decode(output.stdout))
                            : '';
                        stderr = output.stderr
                            ? (typeof output.stderr === 'string' ? output.stderr : dec.decode(output.stderr))
                            : '';
                        exitCode = output.code ?? 0;
                    } catch (e) {
                        stderr = `WASIX exec error (${packageName}): ${e.message}\n`;
                        exitCode = 1;
                    }
                    break;
                }
            }

            // Provide helpful message for unknown commands
            if (packageName) {
                stderr = `'${command}' is available from: ${packageName}\n` +
                         `  SDK status: ${sdkAvailable ? 'ready' : 'not available'}\n` +
                         `  Run: pkg-install ${packageName}\n`;
            } else {
                stderr = `'${command}': command not found\n` +
                         `  Use 'help' for available commands.\n` +
                         `  Use 'pkg-install <user/package>' to install WASIX packages.\n`;
            }
            exitCode = 127;
            break;
        }
    }

    return { stdout, stderr, exitCode };
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
            warn(`stdin write failed for ${id}: ${e.message}`);
        }
    }
}

// ─── Message handler ───────────────────────────────────────

self.onmessage = function(event) {
    const msg = event.data;

    switch (msg.type) {
        case 'Init':
            ensureInit();
            break;

        case 'ExecBash':
            enqueue(() => execBash(msg.id, msg.cmd, msg.timeout_ms || null));
            break;

        case 'ExecPackage':
            enqueue(() => execPackage(msg.id, msg.package, msg.args || [], msg.timeout_ms || null));
            break;

        case 'InstallPackage':
            enqueue(() => handleInstallPackage(msg.id, msg.package));
            break;

        case 'ListPackages':
            handleListPackages(msg.id);
            break;

        case 'CancelExec':
            cancelExec(msg.id);
            break;

        case 'WriteStdin':
            enqueue(() => writeStdin(msg.id, msg.data));
            break;

        default:
            warn(`Unknown command: ${msg.type}`);
    }
};

// Start init eagerly when the worker loads
ensureInit();

log('Shell worker loaded (dynamic WASIX package support)');
