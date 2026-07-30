const assert = require("node:assert/strict");
const vscode = require("vscode");

async function eventually(read, expected, message) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (read() === expected) return;
    await new Promise((resolve) => setTimeout(resolve, 40));
  }
  assert.equal(read(), expected, message);
}

exports.run = async function run() {
  const extension = vscode.extensions.getExtension("shaneyu.stationeers");
  assert(extension, "development extension is discoverable");
  assert(
    vscode.extensions.getExtension("sumneko.lua"),
    "required sumneko.lua dependency is available",
  );
  assert.equal(
    vscode.extensions.getExtension("OrbitalFoundryModdingCrew.stationeers-lua"),
    undefined,
    "StationeersLua VS Code extension is not installed in the isolated host",
  );
  await extension.activate();
  const commands = await vscode.commands.getCommands(true);
  for (const command of [
    "ic10.createEnvironment",
    "ic10.createEnvironmentFromTemplate",
    "ic10.filterTrace",
    "stationeers.live.connect",
    "stationeers.live.pair",
    "stationeers.live.refresh",
  ]) {
    assert(commands.includes(command), `${command} is registered`);
  }

  const uri = vscode.Uri.file(process.env.IC10_EXTENSION_HOST_SCENARIO);
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.commands.executeCommand("vscode.openWith", uri, "ic10.environment");
  await eventually(
    () => vscode.window.tabGroups.activeTabGroup.activeTab?.input?.uri?.toString(),
    uri.toString(),
    "custom topology editor opens and retains focus",
  );

  const original = document.getText();
  const changed = original.replace('"data"', '"renamed-data"');
  const edit = new vscode.WorkspaceEdit();
  edit.replace(
    uri,
    new vscode.Range(document.positionAt(0), document.positionAt(original.length)),
    changed,
  );
  assert(await vscode.workspace.applyEdit(edit), "coherent scenario edit applies");
  await vscode.commands.executeCommand("undo");
  await eventually(() => document.getText(), original, "one undo restores the scenario");
  await vscode.commands.executeCommand("redo");
  await eventually(() => document.getText(), changed, "one redo reapplies the scenario");

  const previousTheme = vscode.workspace
    .getConfiguration("workbench")
    .get("colorTheme");
  await vscode.workspace
    .getConfiguration("workbench")
    .update("colorTheme", "Default High Contrast", vscode.ConfigurationTarget.Global);
  await eventually(
    () => vscode.window.activeColorTheme.kind,
    vscode.ColorThemeKind.HighContrast,
    "topology editor survives a high-contrast theme switch",
  );
  if (previousTheme) {
    await vscode.workspace
      .getConfiguration("workbench")
      .update("colorTheme", previousTheme, vscode.ConfigurationTarget.Global);
  }

  const repositoryRoot = require("node:path").resolve(__dirname, "../../../..");
  const testFixtureUri = vscode.Uri.file(
    require("node:path").join(
      repositoryRoot,
      "examples",
      "multi-ic",
      "ingot-supplier.ictest",
    ),
  );
  await vscode.commands.executeCommand(
    "vscode.openWith",
    testFixtureUri,
    "ic10.scenarioTest",
  );
  await eventually(
    () => vscode.window.tabGroups.activeTabGroup.activeTab?.input?.uri?.toString(),
    testFixtureUri.toString(),
    "custom test editor opens ingot-supplier test fixture",
  );
};
