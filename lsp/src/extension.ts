import * as path from 'path';
import * as fs from 'fs';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let serverPath: string = '';

export async function activate(ctx: vscode.ExtensionContext) {
    // Register all commands first
    registerCommands(ctx);
    
    const config = vscode.workspace.getConfiguration('sald');
    const enabled = config.get<boolean>('lsp.enabled', true);

    if (!enabled) {
        console.log('Sald LSP is disabled');
        return;
    }

    // Get server path from config
    serverPath = config.get<string>('lsp.path', '');
    
    if (!serverPath) {
        vscode.window.showWarningMessage(
            'Sald LSP: No executable path configured',
            'Configure'
        ).then(result => {
            if (result === 'Configure') openLspSettings();
        });
        return;
    }

    if (!fs.existsSync(serverPath)) {
        vscode.window.showErrorMessage(
            `Sald LSP executable not found: ${serverPath}`,
            'Configure'
        ).then(result => {
            if (result === 'Configure') openLspSettings();
        });
        return;
    }

    console.log(`Starting Sald LSP server: ${serverPath}`);
    await startLanguageClient(serverPath);
}

function registerCommands(ctx: vscode.ExtensionContext) {
    // Start server
    ctx.subscriptions.push(
        vscode.commands.registerCommand('sald.startServer', async () => {
            if (client) {
                vscode.window.showWarningMessage('Sald LSP server is already running');
                return;
            }
            
            const config = vscode.workspace.getConfiguration('sald');
            serverPath = config.get<string>('lsp.path', '');
            
            if (!serverPath || !fs.existsSync(serverPath)) {
                vscode.window.showErrorMessage('Please configure the LSP path first');
                openLspSettings();
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
            vscode.window.showInformationMessage('Restarting Sald LSP server...');
            
            if (client) {
                await stopLanguageClient();
            }
            
            const config = vscode.workspace.getConfiguration('sald');
            serverPath = config.get<string>('lsp.path', '');
            
            if (!serverPath || !fs.existsSync(serverPath)) {
                vscode.window.showErrorMessage('Configure LSP path first');
                openLspSettings();
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
}

function openLspSettings() {
    vscode.commands.executeCommand('workbench.action.openSettings', 'sald.lsp.path');
}

async function startLanguageClient(path: string) {
    const serverOptions: ServerOptions = {
        run: { command: path, transport: TransportKind.stdio },
        debug: { command: path, transport: TransportKind.stdio }
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'sald' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.sald')
        },
        diagnosticCollectionName: 'sald',
        diagnosticPullOptions: { onChange: true, onSave: true }
    };

    client = new LanguageClient(
        'saldLanguageServer',
        'Sald Language Server',
        serverOptions,
        clientOptions
    );

    await client.start();
}

async function stopLanguageClient() {
    if (client) {
        await client.stop();
        client = undefined;
    }
}

export function deactivate(): Thenable<void> | undefined {
    return stopLanguageClient();
}
