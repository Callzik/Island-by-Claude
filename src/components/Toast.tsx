import { IAlert, ICheck, ICopy, IInbox, ILauncher } from "./Icons";

export interface ToastData {
  icon: "shelf" | "launcher" | "copy" | "check" | "error";
  title: string;
  subtitle?: string;
  tone?: "error";
}

const ICONS = { shelf: IInbox, launcher: ILauncher, copy: ICopy, check: ICheck, error: IAlert };

export function Toast({ data }: { data: ToastData }) {
  const Icon = ICONS[data.icon];
  return (
    <div className={`toast ${data.tone === "error" ? "toast-error" : ""}`}>
      <div className="toast-icon">
        <Icon size={20} />
      </div>
      <div className="toast-text">
        <div className="toast-title">{data.title}</div>
        {data.subtitle && <div className="toast-sub">{data.subtitle}</div>}
      </div>
    </div>
  );
}
