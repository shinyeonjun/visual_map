export type View = "workbench" | "atlas";

export function ViewSwitch({
  canOpenAtlas = true,
  view,
  setView,
}: {
  canOpenAtlas?: boolean;
  view: View;
  setView: (view: View) => void;
}) {
  const atlasDisabled = !canOpenAtlas && view !== "atlas";
  return (
    <div className="view-switch" role="tablist" aria-label="작업 화면 전환">
      <button
        type="button"
        role="tab"
        data-view="workbench"
        aria-selected={view === "workbench"}
        aria-label="코드와 DB 연결 화면"
        title="프로젝트 코드와 DB 연결"
        className={view === "workbench" ? "active" : ""}
        onClick={() => setView("workbench")}
      >
        코드/DB 연결
      </button>
      {atlasDisabled ? (
        <span className="view-tab locked" title="프로젝트를 열면 근거 캔버스를 볼 수 있습니다">
          근거 캔버스
        </span>
      ) : (
        <button
          type="button"
          role="tab"
          data-view="atlas"
          aria-selected={view === "atlas"}
          aria-label="근거 캔버스 화면"
          title="구조와 영향 범위 캔버스 보기"
          className={view === "atlas" ? "active" : ""}
          onClick={() => setView("atlas")}
        >
          근거 캔버스
        </button>
      )}
    </div>
  );
}
