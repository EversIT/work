// A stand-in for a self-hosted language model (OpenAI chat protocol), for the e2e tests: it answers
// with short fixed texts, chosen by what the system prompt asks for. Started by playwright.config.ts.
import http from 'node:http';

const port = Number(process.env.E2E_FAKE_MODEL_PORT ?? 8089);

function answer(system = '') {
  if (system.includes('Translate the text into German'))
    return 'Die Anmeldung schlägt nach dem Update fehl.';
  if (system.includes('Translate the text'))
    return 'Login fails after the update.';
  if (system.includes('child issues'))
    return '- Write tests\n- Update the docs';
  if (system.includes('propose three different short replies'))
    return '- Thanks, that helps!\n- Can you share the exact version?\n- I will take a look.';
  if (system.includes('polite and constructive')) return 'OK';
  return 'Login fails after the update.';
}

http
  .createServer((req, res) => {
    if (req.method !== 'POST' || req.url !== '/v1/chat/completions') {
      res.writeHead(404).end();
      return;
    }
    let raw = '';
    req.on('data', (c) => (raw += c));
    req.on('end', () => {
      let system = '';
      try {
        system = JSON.parse(raw).messages?.[0]?.content ?? '';
      } catch {
        // an unreadable request still gets a plain answer
      }
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(
        JSON.stringify({
          choices: [
            { message: { role: 'assistant', content: answer(system) } },
          ],
        }),
      );
    });
  })
  .listen(port, '0.0.0.0');
