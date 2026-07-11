import { Terminal } from "lucide-react";
import { useState } from "react";

export type AppPaths = {
  appDataDir: string;
  appStateDb: string;
  enginesDir: string;
  workspacesDir: string;
};

export function DevDiagnostics({ paths, error }: { paths: AppPaths | null; error: string | null }) {
  const [open, setOpen] = useState(false);

  return (
    <span className="dev-diag">
      {open && (
        <div className="dev-diag-panel">
          <strong>개발 경로</strong>
          {paths ? (
            <>
              <span>앱 데이터: {paths.appDataDir}</span>
              <span>앱 상태 DB: {paths.appStateDb}</span>
              <span>읽기 도구 폴더: {paths.enginesDir}</span>
              <span>프로젝트 폴더: {paths.workspacesDir}</span>
            </>
          ) : (
            <span>{error ?? "앱 데이터 경로 확인 중..."}</span>
          )}
        </div>
      )}
      <button
        className={`dev-diag-chip ${open ? "open" : ""}`}
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        title="개발 진단"
      >
        <Terminal size={10} />
        개발
      </button>
    </span>
  );
}
