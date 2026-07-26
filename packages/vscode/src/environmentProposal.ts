import type * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

import {
  EnvironmentProposalPreview,
  buildEnvironmentProposalPreview,
  validateEnvironmentProposal,
} from "./environmentProposalModel";

export const environmentProposalRequest = "ic10/proposeEnvironment";
export const environmentProposalCommand = "ic10.previewEnvironmentProposal";

export class EnvironmentProposalService {
  public constructor(private readonly client: LanguageClient) {}

  public async preview(
    document: vscode.TextDocument,
  ): Promise<EnvironmentProposalPreview> {
    if (document.languageId !== "ic10") {
      throw new Error("Environment proposals require an IC10 document.");
    }
    const sourceUri = document.uri.toString();
    const response = await this.client.sendRequest(environmentProposalRequest, {
      uri: sourceUri,
    });
    return buildEnvironmentProposalPreview(
      validateEnvironmentProposal(response, sourceUri),
    );
  }
}
