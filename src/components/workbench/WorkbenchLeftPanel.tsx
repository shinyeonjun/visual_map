import { ListChecks } from "lucide-react";
import { dbProfileWorkStarted } from "../../types/controls";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { codeInventoryItemCount } from "../../types/workspace";
import { PanelHeader } from "../common/PanelHeader";
import { CodeSourceSection } from "./CodeSourceSection";
import { DatabaseSourceSection } from "./DatabaseSourceSection";
import { WorkspaceCard } from "./WorkspaceCard";

type SetupStepState = "done" | "active" | "optional" | "";

export function WorkbenchLeftPanel({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const setupSteps = workbenchSetupSteps(workspaceControls, dbProfileControls, visualMapControls);
  const showSetupRail = setupSteps.some((step) => step.state === "active" || step.state === "optional");
  const activeStepDetail =
    setupSteps.find((step) => step.state === "active")?.detail ??
    setupSteps.find((step) => step.state === "optional")?.detail ??
    "검색하거나 카드를 선택해 답 기준을 좁히세요.";
  return (
    <aside className="side side-left">
      {showSetupRail && (
        <section className="side-card setup-rail" aria-label="프로젝트 등록 진행 상황">
          <PanelHeader icon={<ListChecks size={16} />} title="코드/DB 연결" />
          <div className="setup-rail-track">
            {setupSteps.map((step, index) => (
              <span
                className={step.state}
                key={step.label}
                aria-current={step.state === "active" ? "step" : undefined}
                title={step.detail}
              >
                <b>{index + 1}</b>
                <em>{step.label}</em>
              </span>
            ))}
          </div>
          <div className="setup-rail-footer">
            <small>{activeStepDetail}</small>
          </div>
        </section>
      )}
      <WorkspaceCard workspaceControls={workspaceControls} />
      {hasWorkspace && (
        <CodeSourceSection workspaceControls={workspaceControls} />
      )}
      <DatabaseSourceSection dbProfileControls={dbProfileControls} />
    </aside>
  );
}

function workbenchSetupSteps(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
): Array<{ label: string; detail: string; state: SetupStepState }> {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasCode = codeInventoryItemCount(workspaceControls.codeInventory) > 0;
  const dbTables = dbProfileControls.inventory?.tables ?? [];
  const hasDbTables = dbTables.length > 0;
  const hasDbColumns = dbTables.some((table) => table.columns.length > 0);
  const hasCompleteDbColumns = hasDbTables && dbTables.every((table) => table.columns.length > 0);
  const dbStarted = dbProfileWorkStarted(dbProfileControls);
  const hasSource = hasCode || hasDbTables;
  const codeOptional = !hasCode && hasDbTables;
  const hasAnswers = Boolean(visualMapControls.currentMap && (visualMapControls.currentMap.nodes.length || visualMapControls.currentMap.edges.length));
  const canFindAnswers = hasCode || hasCompleteDbColumns;
  const dbStepLabel = hasCompleteDbColumns ? "DB" : hasDbTables ? "컬럼" : "DB";
  const activeStep = !hasWorkspace
    ? "workspace"
    : !hasSource
      ? dbStarted || !workspaceControls.canIndexCode
        ? "db"
        : "code"
      : !canFindAnswers
        ? "db"
        : !hasAnswers
          ? "answers"
          : "";

  return [
    {
      label: hasWorkspace ? "열림" : workspaceControls.repoSourceMode === "github" ? "URL" : "폴더",
      detail: hasWorkspace
        ? "분석할 프로젝트가 정해졌습니다."
        : workspaceControls.canCreateWorkspace
          ? "프로젝트를 열면 코드와 DB 연결을 이어갑니다."
          : workspaceControls.repoSourceMode === "github"
            ? "GitHub URL을 입력하면 프로젝트를 열 수 있습니다."
            : "폴더를 지정하면 프로젝트 열기 단계로 이어집니다.",
      state: hasWorkspace ? "done" : activeStep === "workspace" ? "active" : "",
    },
    {
      label: "코드",
      detail: hasCode
        ? "API, 파일, 함수 목록을 읽었습니다."
        : codeOptional
          ? "코드를 연결하면 API/파일 근거까지 붙습니다."
          : "코드를 읽으면 API, 파일, 함수를 찾습니다.",
      state: hasCode ? "done" : activeStep === "code" ? "active" : codeOptional ? "optional" : "",
    },
    {
      label: dbStepLabel,
      detail: hasCompleteDbColumns
        ? "테이블과 컬럼 구조를 읽었습니다."
        : hasDbTables && hasDbColumns
          ? "일부 테이블은 컬럼 구조가 필요합니다."
        : hasDbTables
          ? "테이블만 읽혔습니다. 컬럼을 보강하면 FK/컬럼 근거가 붙습니다."
          : hasCode
            ? "DB 없이도 볼 수 있습니다. 연결하면 FK/컬럼 근거가 붙습니다."
            : "DB를 연결하면 FK/컬럼 근거가 붙습니다.",
      state: hasCompleteDbColumns ? "done" : activeStep === "db" ? "active" : hasCode ? "optional" : "",
    },
    {
      label: "답",
      detail: hasAnswers ? "중앙 답과 오른쪽 대상 근거에서 직접/후보를 확인합니다." : "검색하거나 카드를 선택해 답 기준을 좁힙니다.",
      state: hasAnswers && canFindAnswers ? "done" : activeStep === "answers" ? "active" : "",
    },
  ];
}
