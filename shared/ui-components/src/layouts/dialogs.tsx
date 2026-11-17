// C-05（TA 2026-11-17）: 命令式对话框服务——prompt()/alert()/confirm() 全库退役的
// 共享基座。组件级 Modal（layouts/Modal.tsx）保持;本层在其上提供 promise 化
// confirm/prompt/info 三原语 + 单挂载 DialogHost（module 级 dispatch——任意
// 调用点 `await dialog.ask(...)` 即弹,无需层层传 props）。
//
// a11y: Modal 基座自带焦点陷阱/Esc/aria-modal;prompt 体 Enter 提交/Esc 取消。
import {
  useEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from 'react';
import { Modal } from './Modal';

export interface ConfirmOptions {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}
export interface PromptOptions {
  title: string;
  label?: string;
  initial?: string;
  placeholder?: string;
  multiline?: boolean;
}
export interface InfoOptions {
  title?: string;
  message: string;
}

type Request =
  | { kind: 'confirm'; opts: ConfirmOptions; resolve: (v: boolean) => void }
  | { kind: 'prompt'; opts: PromptOptions; resolve: (v: string | null) => void }
  | { kind: 'info'; opts: InfoOptions; resolve: () => void };

type Dispatch = (r: Request) => void;
let dispatch: Dispatch | null = null;

export const dialog = {
  ask(opts: ConfirmOptions): Promise<boolean> {
    return new Promise((resolve) => {
      if (!dispatch) { resolve(false); return; }
      dispatch({ kind: 'confirm', opts, resolve });
    });
  },
  input(opts: PromptOptions): Promise<string | null> {
    return new Promise((resolve) => {
      if (!dispatch) { resolve(null); return; }
      dispatch({ kind: 'prompt', opts, resolve });
    });
  },
  notify(opts: InfoOptions): Promise<void> {
    return new Promise((resolve) => {
      if (!dispatch) { resolve(); return; }
      dispatch({ kind: 'info', opts, resolve });
    });
  },
};

function Actions({ children }: { children: ReactNode }): ReactElement {
  return <div className="aurora-dlg-actions">{children}</div>;
}

function PromptBody({ opts, resolve }: { opts: PromptOptions; resolve: (v: string | null) => void }): ReactElement {
  const [val, setVal] = useState(opts.initial ?? '');
  const inputRef = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null);
  useEffect(() => { inputRef.current?.focus(); }, []);
  const submit = (): void => { resolve(val); };
  return (
    <>
      {opts.label && <label className="aurora-dlg-label">{opts.label}</label>}
      {opts.multiline ? (
        <textarea
          ref={inputRef as React.RefObject<HTMLTextAreaElement>}
          className="aurora-dlg-input"
          value={val}
          placeholder={opts.placeholder}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') { e.preventDefault(); resolve(null); }
            else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submit(); }
          }}
        />
      ) : (
        <input
          ref={inputRef as React.RefObject<HTMLInputElement>}
          className="aurora-dlg-input"
          value={val}
          placeholder={opts.placeholder}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') { e.preventDefault(); submit(); }
            else if (e.key === 'Escape') { e.preventDefault(); resolve(null); }
          }}
        />
      )}
      <Actions>
        <button type="button" className="aurora-dlg-btn" onClick={() => resolve(null)}>取消</button>
        <button type="button" className="aurora-dlg-btn aurora-dlg-btn-primary" onClick={submit}>确定</button>
      </Actions>
    </>
  );
}

/** 全局唯一挂载点（应用根一次）。同刻仅一活跃请求,后续排队（FIFO）。 */
export function DialogHost(): ReactElement | null {
  const [active, setActive] = useState<Request | null>(null);
  const queue = useRef<Array<Request>>([]);
  const [tick, force] = useState(0);

  useEffect(() => {
    dispatch = (r) => {
      queue.current.push(r);
      force((n) => n + 1);
    };
    return () => { dispatch = null; };
  }, []);

  /* tick 入依赖: 入队（force）后本 effect 须重跑——否则 active 未变时
   * 队列永不 flush（首版实测教训: deps 仅 [active] 时 dialog 请求悬死）。 */
  useEffect(() => {
    if (!active && queue.current.length > 0) {
      setActive(queue.current.shift() ?? null);
    }
  }, [active, tick]);

  if (!active) return null;
  const settle = (resolve: (v: never) => void, value: never): void => {
    resolve(value);
    setActive(null);
  };

  if (active.kind === 'confirm') {
    const { opts, resolve } = active;
    return (
      <Modal open title={opts.title} onClose={() => settle(resolve, false as never)}>
        <p className="aurora-dlg-msg">{opts.message}</p>
        <Actions>
          <button type="button" className="aurora-dlg-btn" onClick={() => settle(resolve, false as never)}>{opts.cancelLabel ?? '取消'}</button>
          <button
            type="button"
            className={`aurora-dlg-btn ${opts.danger ? 'aurora-dlg-btn-danger' : 'aurora-dlg-btn-primary'}`}
            onClick={() => settle(resolve, true as never)}
          >
            {opts.confirmLabel ?? '确定'}
          </button>
        </Actions>
      </Modal>
    );
  }
  if (active.kind === 'prompt') {
    const { opts, resolve } = active;
    return (
      <Modal open title={opts.title} onClose={() => settle(resolve, null as never)}>
        <PromptBody opts={opts} resolve={(v) => settle(resolve, v as never)} />
      </Modal>
    );
  }
  const { opts, resolve } = active;
  return (
    <Modal open title={opts.title ?? '提示'} onClose={() => settle(resolve, undefined as never)}>
      <p className="aurora-dlg-msg">{opts.message}</p>
      <Actions>
        <button type="button" className="aurora-dlg-btn aurora-dlg-btn-primary" onClick={() => settle(resolve, undefined as never)}>知道了</button>
      </Actions>
    </Modal>
  );
}
