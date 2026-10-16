import { useEffect } from 'react'

/**
 * Keeps the closetag server alive while this tab is open.
 * The server exits shortly after the last page disconnects, so we:
 *  - reconnect immediately when the socket drops (page reload, blip)
 *  - send a heartbeat every 10s so the server can detect a dead tab
 */
export function useServerAlive() {
  useEffect(() => {
    let ws: WebSocket | null = null
    let closedByUs = false
    let retry: number | null = null

    const connect = () => {
      if (closedByUs) return
      const proto = location.protocol === 'https:' ? 'wss' : 'ws'
      ws = new WebSocket(`${proto}://${location.host}/ws`)
      ws.onclose = () => {
        if (!closedByUs) {
          retry = window.setTimeout(connect, 600)
        }
      }
      ws.onerror = () => {
        ws?.close()
      }
    }

    connect()
    const heartbeat = window.setInterval(() => {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send('ping')
      }
    }, 10_000)

    return () => {
      closedByUs = true
      if (retry !== null) window.clearTimeout(retry)
      window.clearInterval(heartbeat)
      ws?.close()
    }
  }, [])
}
