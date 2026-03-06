import * as assert from "node:assert/strict";
import * as path from "node:path";

import * as vscode from "vscode";

suite("PalmScript extension", () => {
    const serverPath = path.resolve(
        __dirname,
        "..",
        "..",
        "..",
        "..",
        "target",
        "debug",
        process.platform === "win32" ? "palmscript-lsp.exe" : "palmscript-lsp",
    );

    suiteSetup(async () => {
        await vscode.workspace
            .getConfiguration("palmscript")
            .update("server.path", serverPath, vscode.ConfigurationTarget.Global);
    });

    test("activates and publishes diagnostics for invalid documents", async () => {
        const uri = vscode.Uri.file(
            path.resolve(__dirname, "..", "..", "..", "test-fixtures", "invalid.trl"),
        );
        const document = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(document);
        await waitFor(
            () => vscode.languages.getDiagnostics(uri).length > 0,
            "expected diagnostics",
        );
        assert.ok(vscode.languages.getDiagnostics(uri)[0].message.includes("expected `else`"));
    });

    test("provides hover, definitions, and completions", async () => {
        const uri = vscode.Uri.file(
            path.resolve(__dirname, "..", "..", "..", "test-fixtures", "valid.trl"),
        );
        const document = await vscode.workspace.openTextDocument(uri);
        const editor = await vscode.window.showTextDocument(document);
        const hoverPosition = document.positionAt(document.getText().indexOf("basis"));
        const hovers = (await vscode.commands.executeCommand(
            "vscode.executeHoverProvider",
            uri,
            hoverPosition,
        )) as vscode.Hover[];
        assert.ok(hovers.length > 0);

        const definitionPosition = document.positionAt(document.getText().lastIndexOf("basis"));
        const definitions = (await vscode.commands.executeCommand(
            "vscode.executeDefinitionProvider",
            uri,
            definitionPosition,
        )) as vscode.Location[];
        assert.ok(definitions.length > 0);

        const completionPosition = document.positionAt(document.getText().indexOf("plot"));
        const completions = (await vscode.commands.executeCommand(
            "vscode.executeCompletionItemProvider",
            uri,
            completionPosition,
        )) as vscode.CompletionList;
        assert.ok(completions.items.some((item) => item.label === "ema"));

        const formatted = (await vscode.commands.executeCommand(
            "vscode.executeFormatDocumentProvider",
            uri,
            {},
        )) as vscode.TextEdit[];
        assert.ok(formatted.length > 0);
        void editor;
    });
});

async function waitFor(check: () => boolean, message: string): Promise<void> {
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
        if (check()) {
            return;
        }
        await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(message);
}
