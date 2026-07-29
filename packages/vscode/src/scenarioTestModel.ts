export interface ScenarioTestFixture {
  readonly schemaVersion?: number;
  readonly scenario?: string;
  readonly cases?: readonly ScenarioTestCase[];
}

export type ScenarioLanguage = "ic10" | "lua";
export interface ScenarioProgram {
  readonly id?: string;
  readonly path?: string;
  readonly language?: ScenarioLanguage;
}
export interface ScenarioSource {
  readonly programs?: readonly ScenarioProgram[];
  readonly devices?: readonly {
    readonly ic?: { readonly program?: string };
  }[];
}
export type ScenarioLanguageSummary = "ic10" | "lua" | "mixed" | "unknown";

export function scenarioLanguageSummary(
  scenario: ScenarioSource | undefined,
): ScenarioLanguageSummary {
  const languages = new Set(
    (scenario?.programs ?? [])
      .map((program) => program.language)
      .filter((language): language is ScenarioLanguage =>
        language === "ic10" || language === "lua",
      ),
  );
  if (languages.size === 0) return "unknown";
  if (languages.size > 1) return "mixed";
  return languages.has("lua") ? "lua" : "ic10";
}

export function scenarioLanguageLabel(
  summary: ScenarioLanguageSummary,
  executionKind?: string,
): string {
  if (executionKind === "luaModule") return "Lua module";
  if (summary === "mixed") return "IC10 + Lua";
  if (summary === "lua") return "Lua chip";
  if (summary === "ic10") return "IC10";
  return "Unknown language";
}

export interface ScenarioTestCase {
  readonly name?: string;
  readonly focusIc?: string;
  readonly focusProgram?: string;
  readonly execution?: { readonly kind?: string };
  readonly parameters?: readonly ({ readonly name?: string } & Record<
    string,
    unknown
  >)[];
}

export interface ExpandedCase {
  readonly caseIndex: number;
  readonly parameterIndex?: number;
  readonly caseName: string;
  readonly displayName: string;
  readonly expandedName: string;
  readonly focusIc?: string;
  readonly focusProgram?: string;
  readonly executionKind?: string;
}

export function expandScenarioTestCases(
  fixture: ScenarioTestFixture,
): ExpandedCase[] {
  if (fixture.schemaVersion !== 1 || !Array.isArray(fixture.cases)) {
    return [];
  }
  return fixture.cases.flatMap((testCase, caseIndex) => {
    if (!testCase.name) {
      return [];
    }
    if (!testCase.parameters?.length) {
      return [
        {
          caseIndex,
          caseName: testCase.name,
          displayName: testCase.name,
          expandedName: testCase.name,
          focusIc: testCase.focusIc,
          ...(testCase.focusProgram ? { focusProgram: testCase.focusProgram } : {}),
          ...(testCase.execution?.kind
            ? { executionKind: testCase.execution.kind }
            : {}),
        },
      ];
    }
    return testCase.parameters.map(
      (
        parameter: { readonly name?: string } & Record<string, unknown>,
        parameterIndex: number,
      ) => {
      const values = Object.entries(parameter)
        .filter(([key]) => key !== "name")
        .map(([key, value]) => `${key}=${String(value)}`)
        .join(", ");
      const displayName = parameter.name ?? values;
      return {
        caseIndex,
        parameterIndex,
        caseName: testCase.name!,
        displayName,
        expandedName: `${testCase.name} [${displayName}]`,
        focusIc: testCase.focusIc,
        ...(testCase.focusProgram ? { focusProgram: testCase.focusProgram } : {}),
        ...(testCase.execution?.kind
          ? { executionKind: testCase.execution.kind }
          : {}),
      };
      },
    );
  });
}

export function stringOffset(source: string, value: string): number | undefined {
  const offset = source.indexOf(JSON.stringify(value));
  return offset < 0 ? undefined : offset;
}
