import { AlertTriangle, RefreshCw } from "lucide-react";
import { Component, type ReactNode } from "react";

export class AppErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <main className="app-fatal" role="alert">
        <section className="app-fatal-panel">
          <AlertTriangle size={22} />
          <div>
            <strong>화면을 표시하지 못했습니다</strong>
            <span>프로젝트 원본과 저장된 읽기 결과는 변경하지 않았습니다.</span>
          </div>
          <details>
            <summary>오류 정보</summary>
            <code>{this.state.error.message}</code>
          </details>
          <button className="primary-action" type="button" onClick={() => window.location.reload()}>
            <RefreshCw size={14} />
            앱 다시 불러오기
          </button>
        </section>
      </main>
    );
  }
}
