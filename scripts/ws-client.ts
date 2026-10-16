// Minimal websocket client used by smoke.sh:
// connects to the closetag server, stays alive for `stayMs`, then closes.
const port = Number(Bun.argv[2] ?? 17799)
const stayMs = Number(Bun.argv[3] ?? 1500)

const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`)
let gotPong = false

ws.onopen = () => console.log('ws: open')
ws.onmessage = (e) => {
  if (e.data === 'pong') {
    gotPong = true
    console.log('ws: got pong')
  }
}
ws.onerror = () => {
  console.log('ws: error')
  process.exit(1)
}
ws.onclose = () => {
  console.log(`ws: closed (pong=${gotPong})`)
  process.exit(0)
}

setTimeout(() => {
  ws.send('ping')
  setTimeout(() => ws.close(1000, 'bye'), 400)
}, stayMs)
