const maxMessageBytes = 4096;

async function readMessage(body) {
  if (!body) return new Response('A message is required.', { status: 400 });

  // Enforce the limit even when the client omits Content-Length.
  const reader = body.getReader();
  const bytes = new Uint8Array(maxMessageBytes);
  let length = 0;
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    if (length + value.byteLength > maxMessageBytes) {
      await reader.cancel();
      return new Response('The message exceeds 4096 bytes.', { status: 413 });
    }
    bytes.set(value, length);
    length += value.byteLength;
  }

  let message;
  try {
    message = new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, length)).trim();
  } catch {
    return new Response('Use UTF-8 text.', { status: 400 });
  }
  if (!message) return new Response('A message is required.', { status: 400 });
  return message;
}

export default {
  async fetch(request, env) {
    if (new URL(request.url).pathname !== '/v1/feedback') {
      return new Response(null, { status: 404 });
    }
    if (request.method !== 'POST') {
      return new Response(null, { status: 405, headers: { Allow: 'POST' } });
    }
    const mediaType = request.headers.get('Content-Type')?.split(';')[0].trim().toLowerCase();
    if (mediaType !== 'text/plain') {
      return new Response('Use text/plain with UTF-8 text.', { status: 415 });
    }
    const message = await readMessage(request.body);
    if (message instanceof Response) return message;

    try {
      await env.DB.prepare('INSERT INTO feedback (message) VALUES (?)').bind(message).run();
    } catch {
      return new Response('The inbox is unavailable.', { status: 503 });
    }
    return new Response(null, { status: 201 });
  }
};
