import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import * as https from 'https';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';

const REPO = 'starssxhfdmh/sald';
const GITHUB_API = `https://api.github.com/repos/${REPO}/releases/latest`;

let client: LanguageClient | undefined;
let outputChannel: vscode.OutputChannel;

/**
 * Get the default sald-lsp binary path based on OS
 */
function getDefaultLspPath(): string {
    const homeDir = os.homedir();
    const platform = os.platform();
    const binName = platform === 'win32' ? 'sald-lsp.exe' : 'sald-lsp';
    return path.join(homeDir, '.sald', 'bin', binName);
}

/**
 * Get platform identifier for download URL
 */
function getPlatformIdentifier(): string {
    const platform = os.platform();
    const arch = os.arch();
    
    if (platform === 'win32') {
        return 'windows-x86_64';
    } else if (platform === 'darwin') {
        return arch === 'arm64' ? 'macos-aarch64' : 'macos-x86_64';
    } else {
        return 'linux-x86_64';
    }
}

/**
 * Fetch JSON from URL
 */
function fetchJson(url: string): Promise<any> {
    return new Promise((resolve, reject) => {
        const options = {
            headers: {
                'User-Agent': 'sald-vscode-extension'
            }
        };
        
        https.get(url, options, (res) => {
            // Handle redirects
            if (res.statusCode === 301 || res.statusCode === 302) {
                const redirectUrl = res.headers.location;
                if (redirectUrl) {
                    return fetchJson(redirectUrl).then(resolve).catch(reject);
                }
            }
            
            if (res.statusCode !== 200) {
                reject(new Error(`HTTP ${res.statusCode}`));
                return;
            }
            
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try {
                    resolve(JSON.parse(data));
                } catch (e) {
                    reject(e);
                }
            });
        }).on('error', reject);
    });
}

/**
 * Download file with progress
 */
function downloadFile(url: string, destPath: string, progress: vscode.Progress<{ message?: string; increment?: number }>): Promise<void> {
    return new Promise((resolve, reject) => {
        const options = {
            headers: {
                'User-Agent': 'sald-vscode-extension'
            }
        };
        
        const download = (targetUrl: string) => {
            https.get(targetUrl, options, (res) => {
                // Handle redirects
                if (res.statusCode === 301 || res.statusCode === 302) {
                    const redirectUrl = res.headers.location;
                    if (redirectUrl) {
                        return download(redirectUrl);
                    }
                }
                
                if (res.statusCode !== 200) {
                    reject(new Error(`HTTP ${res.statusCode}`));
                    return;
                }
                
                const totalSize = parseInt(res.headers['content-length'] || '0', 10);
                let downloadedSize = 0;
                let lastPercent = 0;
                
                // Ensure directory exists
                const dir = path.dirname(destPath);
                if (!fs.existsSync(dir)) {
                    fs.mkdirSync(dir, { recursive: true });
                }
                
                const file = fs.createWriteStream(destPath);
                
                res.on('data', (chunk) => {
                    downloadedSize += chunk.length;
                    if (totalSize > 0) {
                        const percent = Math.floor((downloadedSize / totalSize) * 100);
                        if (percent > lastPercent) {
                            progress.report({ 
                                message: `${percent}%`,
                                increment: percent - lastPercent
                            });
                            lastPercent = percent;
                        }
                    }
                });
                
                res.pipe(file);
                
                file.on('finish', () => {
                    file.close();
                    // Make executable on Unix
                    if (os.platform() !== 'win32') {
                        fs.chmodSync(destPath, 0o755);
                    }
                    resolve();
                });
                
                file.on('error', (err) => {
                    fs.unlink(destPath, () => {});
                    reject(err);
                });
            }).on('error', reject);
        };
        
        download(url);
    });
}

/**
 * Download the latest sald-lsp binary
 */
async function downloadLsp(destPath: string): Promise<boolean> {
    return vscode.window.withProgress({
        location: vscode.ProgressLocation.Notification,
        title: 'Sald LSP',
        cancellable: false
    }, async (progress) => {
        try {
            progress.report({ message: 'Fetching latest version...' });
            
            const release = await fetchJson(GITHUB_API);
            const version = release.tag_name;
            
            if (!version) {
                throw new Error('Could not determine latest version');
            }
            
            log(`Latest version: ${version}`);
            
            const platform = getPlatformIdentifier();
            const ext = os.platform() === 'win32' ? '.exe' : '';
            const fileName = `sald-lsp-${platform}${ext}`;
            const downloadUrl = `https://github.com/${REPO}/releases/download/${version}/${fileName}`;
            
            log(`Downloading from: ${downloadUrl}`);
            progress.report({ message: `Downloading ${version}...` });
            
            await downloadFile(downloadUrl, destPath, progress);
            
            log(`Downloaded to: ${destPath}`);
            vscode.window.showInformationMessage(`Sald LSP ${version} installed successfully!`);
            
            return true;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            log(`Download failed: ${message}`);
            vscode.window.showErrorMessage(`Failed to download Sald LSP: ${message}`);
            return false;
        }
    });
}

/**
 * Find or download the LSP binary
 */
async function findOrDownloadLsp(): Promise<string | null> {
    const config = vscode.workspace.getConfiguration('sald');
    
    // 1. Check user-configured path first
    const configuredPath = config.get<string>('lsp.path', '');
    if (configuredPath && fs.existsSync(configuredPath)) {
        log(`Using configured LSP path: ${configuredPath}`);
        return configuredPath;
    }
    
    // 2. Check default path (~/.sald/bin/sald-lsp)
    const defaultPath = getDefaultLspPath();
    if (fs.existsSync(defaultPath)) {
        log(`Found LSP at default path: ${defaultPath}`);
        return defaultPath;
    }
    
    // 3. Check if sald-lsp is in PATH
    const pathLsp = os.platform() === 'win32' ? 'sald-lsp.exe' : 'sald-lsp';
    // We skip PATH check for simplicity - rely on default location
    
    // 4. Not found - ask user to download
    log('LSP binary not found, prompting for download...');
    
    const action = await vscode.window.showInformationMessage(
        'Sald LSP binary not found. Would you like to download it automatically?',
        'Download',
        'Configure Path',
        'Cancel'
    );
    
    if (action === 'Download') {
        const success = await downloadLsp(defaultPath);
        if (success) {
            return defaultPath;
        }
    } else if (action === 'Configure Path') {
        openLspSettings();
    }
    
    return null;
}

/**
 * Log message to output channel
 */
function log(message: string) {
    const timestamp = new Date().toISOString();
    outputChannel.appendLine(`[${timestamp}] ${message}`);
}

export async function activate(ctx: vscode.ExtensionContext) {
    // Create output channel
    outputChannel = vscode.window.createOutputChannel('Sald');
    ctx.subscriptions.push(outputChannel);
    
    log('Sald extension activating...');
    
    // Register all commands
    registerCommands(ctx);
    
    const config = vscode.workspace.getConfiguration('sald');
    const enabled = config.get<boolean>('lsp.enabled', true);

    if (!enabled) {
        log('Sald LSP is disabled in settings');
        return;
    }

    // Find or download LSP binary
    const serverPath = await findOrDownloadLsp();
    
    if (!serverPath) {
        log('No LSP binary available, extension running in limited mode');
        return;
    }

    log(`Starting LSP server: ${serverPath}`);
    await startLanguageClient(serverPath);
}

function registerCommands(ctx: vscode.ExtensionContext) {
    // Show logs
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.showLogs', () => {
            outputChannel.show();
        })
    );
    
    // Start server
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.startServer', async () => {
            if (client) {
                vscode.window.showWarningMessage('Sald LSP server is already running');
                return;
            }
            
            const serverPath = await findOrDownloadLsp();
            if (!serverPath) {
                return;
            }
            
            await startLanguageClient(serverPath);
            vscode.window.showInformationMessage('Sald LSP server started');
        })
    );
    
    // Stop server
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.stopServer', async () => {
            if (!client) {
                vscode.window.showWarningMessage('Sald LSP server is not running');
                return;
            }
            
            await stopLanguageClient();
            vscode.window.showInformationMessage('Sald LSP server stopped');
        })
    );
    
    // Restart server
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.restartServer', async () => {
            log('Restarting LSP server...');
            vscode.window.showInformationMessage('Restarting Sald LSP server...');
            
            if (client) {
                await stopLanguageClient();
            }
            
            const serverPath = await findOrDownloadLsp();
            if (!serverPath) {
                return;
            }
            
            await startLanguageClient(serverPath);
            vscode.window.showInformationMessage('Sald LSP server restarted');
        })
    );
    
    // Configure path
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.configurePath', () => openLspSettings())
    );
    
    // Analyze all files
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.showAllDiagnostics', async () => {
            if (!client) {
                vscode.window.showWarningMessage('Sald LSP server is not running');
                return;
            }
            
            const files = await vscode.workspace.findFiles('**/*.sald', '**/sald_modules/**');
            for (const file of files) {
                await vscode.workspace.openTextDocument(file);
            }
            vscode.window.showInformationMessage(`Analyzed ${files.length} .sald files`);
        })
    );
    
    // Run current file
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.runFile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'sald') {
                vscode.window.showWarningMessage('No Sald file is open');
                return;
            }
            
            // Save file first
            await editor.document.save();
            
            const terminal = vscode.window.createTerminal('Sald');
            terminal.sendText(`sald "${editor.document.fileName}"`);
            terminal.show();
        })
    );
    
    // Update LSP
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.updateLsp', async () => {
            const defaultPath = getDefaultLspPath();
            
            // Stop server if running
            if (client) {
                await stopLanguageClient();
            }
            
            const success = await downloadLsp(defaultPath);
            if (success) {
                // Restart server with new binary
                await startLanguageClient(defaultPath);
            }
        })
    );
}

function openLspSettings() {
    vscode.commands.executeCommand('workbench.action.openSettings', 'sald.lsp.path');
}

async function startLanguageClient(serverPath: string) {
    const serverOptions: ServerOptions = {
        run: { command: serverPath, transport: TransportKind.stdio },
        debug: { command: serverPath, transport: TransportKind.stdio }
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'sald' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.sald')
        },
        diagnosticCollectionName: 'sald',
        diagnosticPullOptions: { onChange: true, onSave: true },
        outputChannel: outputChannel
    };

    client = new LanguageClient(
        'saldLanguageServer',
        'Sald Language Server',
        serverOptions,
        clientOptions
    );

    await client.start();
    log('LSP server started successfully');
}

async function stopLanguageClient() {
    if (client) {
        await client.stop();
        client = undefined;
        log('LSP server stopped');
    }
}

export function deactivate(): Thenable<void> | undefined {
    return stopLanguageClient();
}
