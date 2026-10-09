import { IInbox, ILauncher } from "./Icons";

export type DropZone = "shelf" | "launcher";

export function DropZones({ zone, count }: { zone: DropZone | null; count: number }) {
  return (
    <div className="drop">
      <div className={`drop-zone ${zone === "shelf" ? "active" : ""}`}>
        <IInbox size={26} />
        <div className="drop-title">На полку</div>
        <div className="drop-sub">{count > 1 ? `${count} файла(ов) полежат, пока не понадобятся` : "полежит, пока не понадобится"}</div>
      </div>
      <div className={`drop-zone ${zone === "launcher" ? "active" : ""}`}>
        <ILauncher size={26} />
        <div className="drop-title">В лаунчер</div>
        <div className="drop-sub">ярлык для быстрого запуска</div>
      </div>
    </div>
  );
}
