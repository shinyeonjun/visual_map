import { ArrowDownToLine, ArrowUpFromLine, KeyRound, Link2, Table2, TriangleAlert } from "lucide-react";
import type { VisualMap, VisualNode } from "../../types/visual-map";
import type { VisualMapControls } from "../../types/controls";
import type { ErdNeighbor, TableErdModel } from "./tableErdModel";
import type { ColumnImpactCascadeModel } from "./columnImpactModel";
import { relationSourceLabel } from "./relationMeta";

/** Mini ERD: FK neighbors around the focused table with code usage badges. */
export function TableErd({
  model,
  visualMapControls,
}: {
  model: TableErdModel;
  visualMapControls: VisualMapControls;
}) {
  return (
    <section className="table-erd" aria-label={`${model.tableLabel} 구조 지도`}>
      <div className="table-erd-columns">
        <div className="table-erd-side" aria-label="이 테이블을 참조하는 테이블">
          <span className="table-erd-side-title">이 테이블을 참조</span>
          {model.inbound.length === 0 ? <p className="table-erd-empty">참조하는 테이블이 없습니다.</p> : null}
          {model.inbound.map((neighbor) => (
            <ErdNeighborCard neighbor={neighbor} visualMapControls={visualMapControls} key={neighbor.id} />
          ))}
          {model.hiddenInbound > 0 ? <p className="table-erd-more">+{model.hiddenInbound}개 더</p> : null}
        </div>

        <div className="table-erd-focus-wrap">
          <article className="table-erd-focus" aria-label={`${model.tableLabel} 테이블`}>
            <header>
              <Table2 size={15} />
              <strong>{model.tableLabel}</strong>
            </header>
            <ul>
              {model.columns.map((column) => (
                <li key={column.name}>
                  {column.isPrimaryKey ? <KeyRound size={11} aria-label="기본 키" /> : column.isForeignKey ? <Link2 size={11} aria-label="외래 키" /> : <i aria-hidden="true" />}
                  <code>{column.name}</code>
                  {column.dataType ? <small>{column.dataType}</small> : null}
                </li>
              ))}
            </ul>
            {model.hiddenColumns > 0 ? <p>+{model.hiddenColumns}개 컬럼</p> : null}
            <footer aria-label="코드 사용 근거">
              <span className={model.reads > 0 ? "confirmed" : "quiet"}><ArrowDownToLine size={12} />READS {model.reads}</span>
              <span className={model.writes > 0 ? "confirmed" : "quiet"}><ArrowUpFromLine size={12} />WRITES {model.writes}</span>
              {model.candidateUses > 0 ? <span className="candidate"><TriangleAlert size={12} />후보 {model.candidateUses}</span> : null}
            </footer>
          </article>
        </div>

        <div className="table-erd-side" aria-label="이 테이블이 참조하는 테이블">
          <span className="table-erd-side-title">이 테이블이 참조</span>
          {model.outbound.length === 0 ? <p className="table-erd-empty">참조하는 대상이 없습니다.</p> : null}
          {model.outbound.map((neighbor) => (
            <ErdNeighborCard neighbor={neighbor} visualMapControls={visualMapControls} key={neighbor.id} />
          ))}
          {model.hiddenOutbound > 0 ? <p className="table-erd-more">+{model.hiddenOutbound}개 더</p> : null}
        </div>
      </div>
    </section>
  );
}

function ErdNeighborCard({
  neighbor,
  visualMapControls,
}: {
  neighbor: ErdNeighbor;
  visualMapControls: VisualMapControls;
}) {
  const body = (
    <>
      <span className="table-erd-neighbor-name"><Table2 size={13} />{neighbor.label}</span>
      <code className="table-erd-neighbor-via" title={`FK ${neighbor.viaLabel}`}>{neighbor.viaLabel}</code>
    </>
  );
  return neighbor.nodeId ? (
    <button
      className={`table-erd-neighbor ${neighbor.direction}`}
      type="button"
      title={`${neighbor.label} 테이블 연결 보기`}
      onClick={() => visualMapControls.showMode("table-usage", neighbor.nodeId!)}
    >
      {body}
    </button>
  ) : (
    <div className={`table-erd-neighbor ${neighbor.direction} static`}>{body}</div>
  );
}

/** Left-to-right impact cascade for a column: DB structure → code → API. */
export function ColumnImpactCascade({
  model,
  map,
  visualMapControls,
}: {
  model: ColumnImpactCascadeModel;
  map: VisualMap;
  visualMapControls: VisualMapControls;
}) {
  return (
    <section className="impact-cascade" aria-label={`${model.subject} 영향 전파`}>
      <article className="impact-cascade-subject">
        <span>변경 대상</span>
        <strong>{model.subject}</strong>
      </article>
      {model.tiers.map((tier) => (
        <div className="impact-cascade-tier" key={tier.id}>
          <span className="impact-cascade-arrow" aria-hidden="true">→</span>
          <div className="impact-cascade-column">
            <header>
              <strong>{tier.title}</strong>
              <small>{tier.description}</small>
            </header>
            {tier.items.map((item) => {
              const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
              const location = relationSourceLabel(item.location);
              const content = (
                <>
                  <strong>{item.title}</strong>
                  <small title={location ?? item.detail}>{location ?? item.detail}</small>
                </>
              );
              return node ? (
                <button
                  className={`impact-cascade-item ${item.truthClass}`}
                  type="button"
                  onClick={() => selectCascadeNode(node, visualMapControls)}
                  key={item.id}
                >
                  {content}
                </button>
              ) : (
                <div className={`impact-cascade-item ${item.truthClass} static`} key={item.id}>{content}</div>
              );
            })}
            {tier.hidden > 0 ? <p className="impact-cascade-more">+{tier.hidden}개 더 · 아래 목록 참고</p> : null}
          </div>
        </div>
      ))}
    </section>
  );
}

function selectCascadeNode(node: VisualNode, visualMapControls: VisualMapControls): void {
  visualMapControls.selectNode(node);
}
