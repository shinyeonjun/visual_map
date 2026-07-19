import { CheckCircle2, Database, FolderOpen, Layers3 } from "lucide-react";
import { tauriUnavailableMessage } from "../../app/tauriRuntime";
import {
  codeInventoryCodeItems,
  codeInventoryDefaultRoute,
  codeInventoryItemCount,
  dbInventoryTableKey,
} from "../../types/workspace";
import {
  dbProfileWorkStarted,
  type DbProfileControls,
  type VisualMapControls,
  type WorkspaceControls,
} from "../../types/controls";
import {
  focusDbProfileSetup,
  focusSourceSetup,
  focusWorkspaceSetup,
} from "../common/focusSourceSetup";

export function SetupChecklist({
  title,
  openSourceManager,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  title: string;
  openSourceManager: () => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const codeCount = codeInventoryItemCount(workspaceControls.codeInventory);
  const routeCount = workspaceControls.codeInventory?.routes.length ?? 0;
  const codeSymbolCount = codeInventoryCodeItems(workspaceControls.codeInventory).length;
  const fileCount = workspaceControls.codeInventory?.files.length ?? 0;
  const dbTables = dbProfileControls.inventory?.tables ?? [];
  const dbCount = dbTables.length;
  const dbColumnCount = dbTables.reduce((sum, table) => sum + table.columns.length, 0);
  const dbMissingColumnTables = dbTables.filter((table) => table.columns.length === 0).length;
  const dbReady = dbCount > 0 && dbColumnCount > 0 && dbMissingColumnTables === 0;
  const hasCodeContext = routeCount > 0 || codeSymbolCount > 0;
  const codeIndexed = workspaceControls.codeStatus?.includes("완료") ?? false;
  const dbStarted = dbProfileWorkStarted(dbProfileControls);
  const canUseCodeStep = codeIndexed || workspaceControls.canIndexCode;
  const projectSourceLabel = workspaceControls.repoSourceMode === "github" ? "GitHub 저장소" : "프로젝트 폴더";
  const projectStepLabel = workspaceControls.repoSourceMode === "github" ? "저장소 연결" : "프로젝트 열기";
  const projectStepAction = workspaceControls.canCreateWorkspace
    ? workspaceControls.repoSourceMode === "github"
      ? "복제하고 열기"
      : "프로젝트 열기"
    : workspaceControls.repoSourceMode === "github"
      ? "URL 입력"
      : "폴더 선택";
  const codeStepAction = codeIndexed ? "목록 열기" : workspaceControls.canIndexCode ? "코드 읽기" : "코드 섹션";
  const dbStepAction = dbProfileControls.canIndexProfile
    ? "DB 읽기"
    : dbProfileControls.canLoadInventory
      ? "목록 열기"
      : "DB 섹션";
  const codeReadyText = codeInventoryReadyText({
    routes: routeCount,
    code: codeSymbolCount,
    files: fileCount,
  });
  const steps = [
    {
      icon: FolderOpen,
      label: projectStepLabel,
      text: hasWorkspace
        ? workspaceControls.currentWorkspace?.name ?? "연결됨"
        : workspaceControls.canCreateWorkspace
          ? `${workspaceControls.workspaceName || projectSourceLabel} ${workspaceControls.repoSourceMode === "github" ? "복제 준비됨" : "열기 준비됨"}`
          : workspaceControls.repoSourceMode === "github"
            ? "GitHub URL 입력"
          : "로컬 폴더를 지정하세요",
      feedback: workspaceControls.error ?? null,
      done: hasWorkspace,
      place: projectStepAction,
      run: workspaceControls.canCreateWorkspace
        ? workspaceControls.createWorkspace
        : workspaceControls.repoSourceMode === "local"
          ? workspaceControls.pickRepoPath
          : () => {
              openSourceManager();
              focusWorkspaceSetup(workspaceControls);
            },
      disabled: workspaceControls.busy,
    },
    {
      icon: Layers3,
      label: "코드 목록",
      text: codeCount > 0 ? codeReadyText : "API, 코드, 파일 읽기",
      feedback: workspaceControls.codeError ?? null,
      done: codeCount > 0,
      place: codeStepAction,
      run: codeIndexed
        ? workspaceControls.loadCodeInventory
        : workspaceControls.canIndexCode
        ? workspaceControls.indexCodeRepository
        : () => focusSourceSetup(openSourceManager, workspaceControls, dbProfileControls),
      disabled: workspaceControls.busy || !hasWorkspace,
    },
    {
      icon: Database,
      label: "DB 구조",
      text: dbReady
        ? `테이블 ${dbCount}개 · 컬럼 ${dbColumnCount}개 읽힘`
        : dbMissingColumnTables > 0 && dbColumnCount > 0
          ? `테이블 ${dbCount}개 · ${dbMissingColumnTables}개 컬럼 보강`
        : dbCount > 0
          ? `테이블 ${dbCount}개 · 컬럼 대기`
          : hasCodeContext
            ? "변경 범위 읽기"
          : "테이블/컬럼 읽기",
      feedback: dbProfileControls.error ?? null,
      done: dbReady,
      place: dbStepAction,
      run: dbProfileControls.canIndexProfile
        ? dbProfileControls.indexProfile
        : dbProfileControls.canLoadInventory
          ? dbProfileControls.loadInventory
          : () => showWorkbenchDbSetup(openSourceManager, dbProfileControls),
      disabled: dbProfileControls.busy || !hasWorkspace,
    },
  ];
  const defaultRoute = codeInventoryDefaultRoute(
    workspaceControls.codeInventory,
    workspaceControls.selectedCodeItem?.id,
  );
  const firstCodeItem =
    defaultRoute ??
    codeInventoryCodeItems(workspaceControls.codeInventory)[0] ??
    workspaceControls.codeInventory?.files[0] ??
    null;
  const firstTableKey = dbTables[0] ? dbInventoryTableKey(dbTables[0]) : null;
  const firstColumnFocus =
    dbTables
      .map((table) => {
        const column = table.columns.find((item) => item.isForeignKey) ?? table.columns[0] ?? null;
        return column ? `db:column:${dbInventoryTableKey(table)}:${column.name}` : null;
      })
      .find(Boolean) ?? null;
  const runCodeAnswer = () => {
    if (!hasWorkspace) {
      steps[0].run();
      return;
    }
    if (defaultRoute) {
      visualMapControls.showMode("api-flow", `code:${defaultRoute.id}`);
      return;
    }
    if (firstCodeItem) {
      visualMapControls.showMode("search-focus", `code:${firstCodeItem.id}`);
      return;
    }
    steps[1].run();
  };
  const runTableAnswer = () => {
    if (!hasWorkspace) {
      steps[0].run();
      return;
    }
    if (firstTableKey) {
      visualMapControls.showMode("table-usage", `db:table:${firstTableKey}`);
      return;
    }
    steps[2].run();
  };
  const runImpactAnswer = () => {
    if (!hasWorkspace) {
      steps[0].run();
      return;
    }
    if (firstColumnFocus) {
      visualMapControls.showMode("column-impact", firstColumnFocus);
      return;
    }
    steps[2].run();
  };
  const activeStep = !hasWorkspace
    ? 0
    : codeCount === 0 && !dbStarted && canUseCodeStep
      ? 1
      : !dbReady
        ? 2
        : -1;
  const setupSummary = !hasWorkspace
    ? workspaceControls.canCreateWorkspace
      ? workspaceControls.repoSourceMode === "github"
        ? "복제하면 API, 코드, DB 답이 열립니다."
        : "열면 API, 코드, DB 답이 열립니다."
      : workspaceControls.repoSourceMode === "github"
        ? "GitHub URL을 넣으면 코드와 DB 답을 엽니다."
        : "폴더를 연결하면 코드와 DB 답이 열립니다."
    : dbCount > 0 && !dbReady
    ? dbColumnCount > 0
      ? "일부 테이블은 컬럼을 더 읽어야 해당 제약과 변경 영향이 열립니다."
      : "테이블 목록은 읽혔고, 컬럼을 읽으면 제약과 변경 영향이 열립니다."
    : hasCodeContext
      ? routeCount > 0
        ? "코드와 DB 구조를 읽으면 API 경로, DB 제약, 후보 근거가 채워집니다."
        : "코드와 DB 구조를 읽으면 파일/심볼 구조, DB 제약, 후보 근거가 채워집니다."
      : "DB 구조는 테이블 구조와 컬럼 제약부터 보여주고, 코드 목록이 연결되면 후보 근거까지 확장됩니다.";
  const heroTitle = hasWorkspace ? title : `${projectSourceLabel} 연결`;
  const codeAnswerState = hasCodeContext || fileCount > 0 ? "ready" : "pending";
  const tableAnswerState = dbCount > 0 ? "ready" : "pending";
  const impactAnswerState = dbReady ? "ready" : dbColumnCount > 0 ? "partial" : "pending";
  const answerTitle = hasWorkspace ? "눌러서 바로 찾기" : "연결 후 찾을 답";
  const projectNeededSource = workspaceControls.repoSourceMode === "github" ? "URL 먼저" : "폴더 먼저";
  const codeNeededSource = hasWorkspace ? "코드 읽기 먼저" : projectNeededSource;
  const dbNeededSource = hasWorkspace ? "DB 연결 먼저" : projectNeededSource;
  const codeAnswerPreview =
    routeCount > 0
      ? { label: "API 흐름", question: "이 API는 어디서 처리돼?", source: "라우트 + 코드", state: codeAnswerState }
      : codeSymbolCount > 0
        ? { label: "코드 근거", question: "이 코드는 어디서 불려?", source: "심볼 + 파일", state: codeAnswerState }
        : fileCount > 0
          ? { label: "파일 근거", question: "이 파일은 어디와 묶여?", source: "파일 목록", state: codeAnswerState }
        : hasWorkspace
          ? { label: "코드 목록", question: "읽힌 코드가 뭐야?", source: codeNeededSource, state: codeAnswerState }
          : { label: "API 흐름", question: "이 API는 어디서 처리돼?", source: projectNeededSource, state: codeAnswerState };
  const answerPreviews = [
    { ...codeAnswerPreview, run: runCodeAnswer },
    dbCount > 0 && !dbReady
      ? { label: "테이블 목록", question: "읽힌 테이블 확인", source: "DB 구조", state: tableAnswerState, run: runTableAnswer }
      : dbCount === 0
        ? { label: "테이블 연결", question: "이 테이블은 누가 써?", source: dbNeededSource, state: tableAnswerState, run: runTableAnswer }
      : hasCodeContext
        ? { label: "테이블 연결", question: "이 테이블은 누가 써?", source: "코드 + DB", state: tableAnswerState, run: runTableAnswer }
          : { label: "테이블 구조", question: "PK/FK가 어떻게 묶여?", source: "DB 구조", state: tableAnswerState, run: runTableAnswer },
    dbCount > 0 && !dbReady
      ? { label: "컬럼 보강", question: "컬럼 읽기 필요", source: dbColumnCount > 0 ? "컬럼 보강 필요" : "컬럼 읽기 필요", state: impactAnswerState, run: runImpactAnswer }
      : dbCount === 0
        ? { label: "변경 범위", question: "이 컬럼 바꾸면 어디까지 닿아?", source: dbNeededSource, state: impactAnswerState, run: runImpactAnswer }
      : hasCodeContext
        ? { label: "변경 범위", question: "이 컬럼 바꾸면 어디까지 닿아?", source: "컬럼 + 근거", state: impactAnswerState, run: runImpactAnswer }
          : { label: "컬럼 제약", question: "컬럼 제약 구조", source: "컬럼 + 제약", state: impactAnswerState, run: runImpactAnswer },
  ];
  const activeStepIndex = activeStep >= 0 ? activeStep : steps.findIndex((step) => !step.done);

  return (
    <div className="map-empty setup-empty">
      <div className="setup-hero">
        <Layers3 size={28} />
        <strong>{heroTitle}</strong>
        <span>{setupSummary}</span>
      </div>
      <div className="setup-body">
        <div className="setup-start">
          <span className="setup-answer-title">연결 순서</span>
          <div className="setup-steps" aria-label="프로젝트 연결 순서">
            {steps.map((step, index) => {
              const StepIcon = step.icon;
              const stepActive = index === activeStepIndex && !step.done;
              return (
                <div
                  className={`setup-step ${step.done ? "done" : ""} ${stepActive ? "active primary" : ""}`}
                  aria-current={stepActive ? "step" : undefined}
                  key={step.label}
                >
                  <span className="setup-state">{step.done ? <CheckCircle2 size={15} /> : index + 1}</span>
                  <StepIcon size={16} />
                  <span className="setup-copy">
                    <b>{step.label}</b>
                    <small>{step.text}</small>
                    {step.feedback && (
                      <small className={`setup-feedback ${step.feedback === tauriUnavailableMessage ? "notice" : "error"}`}>
                        {step.feedback}
                      </small>
                    )}
                  </span>
                  {step.done ? (
                    <span className="setup-place">완료</span>
                  ) : stepActive ? (
                    <button
                      className="setup-place action"
                      type="button"
                      onClick={step.run}
                      onPointerDown={(event) => event.stopPropagation()}
                      disabled={step.disabled}
                    >
                      {step.place}
                    </button>
                  ) : (
                    <span className="setup-place waiting">대기</span>
                  )}
                </div>
              );
            })}
          </div>
        </div>
        <div className="setup-answers" aria-label="코드/DB 연결 후 확인할 수 있는 답">
          <span className="setup-answer-title">{answerTitle}</span>
          {answerPreviews.map((item) => (
            <button
              className={`setup-answer ${item.state}`}
              key={item.label}
              type="button"
              aria-label={`${item.label}: ${item.question} · ${item.source}`}
              title={`${item.question} · ${item.source}`}
              onClick={item.run}
              onPointerDown={(event) => event.stopPropagation()}
            >
              <small>{item.label}</small>
              <b>{item.question}</b>
              <em>{item.source}</em>
            </button>
          ))}
        </div>
      </div>
    </div>
  );

}

function showWorkbenchDbSetup(openSourceManager: () => void, dbProfileControls: DbProfileControls) {
  openSourceManager();
  focusDbProfileSetup(dbProfileControls);
}

function codeInventoryReadyText(counts: {
  routes: number;
  code: number;
  files: number;
}): string {
  const parts = [
    counts.routes > 0 ? `API ${counts.routes}개` : null,
    counts.code > 0 ? `코드 ${counts.code}개` : null,
    counts.files > 0 ? `파일 ${counts.files}개` : null,
  ].filter(Boolean);
  return parts.length > 0 ? `${parts.join(" · ")} 읽힘` : "코드 목록 읽힘";
}
