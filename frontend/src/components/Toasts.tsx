import { useApp } from '../context'

export default function Toasts() {
  const { toasts, dismissToast } = useApp()
  if (toasts.length === 0) return null
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`toast ${t.kind === 'err' ? 'toast-err' : 'toast-ok'}`}
          onClick={() => dismissToast(t.id)}
          title="点击关闭"
        >
          {t.text}
        </div>
      ))}
    </div>
  )
}
