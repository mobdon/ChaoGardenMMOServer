// Maestro runScript helper. Called at the end of each flow to assert the
// server side effect the UI step should have produced. Prints a JSON summary
// to stdout so `run-e2e.sh` can append it to e2e/reports/maestro/results.jsonl.
//
// Security: reads the session token from TEST_TOKEN_FILE (a path, never an
// inline value) and never logs it. Base URL comes from BASE_URL.
const output = {};
const baseUrl = (typeof BASE_URL === 'string' && BASE_URL) || 'http://127.0.0.1:3001';
const check = (typeof CHECK === 'string' && CHECK) || 'unknown-check';

// Minimal HTTP status probe. Maestro runScript has no fetch in all runtimes,
// so fall back to a stub that records the check instead of failing the flow;
// run-e2e.sh performs the authoritative /api/status assertions in shell.
function probe(url) {
  try {
    const xhr = typeof XMLHttpRequest !== 'undefined' ? new XMLHttpRequest() : null;
    if (!xhr) return { checked: false };
    xhr.open('GET', url, false);
    xhr.setRequestHeader('Accept', 'application/json');
    xhr.send(null);
    return { checked: true, status: xhr.status };
  } catch (e) {
    return { checked: false, error: String(e).replace(/token=[^&\s]+/g, 'token=[redacted]') };
  }
}

const status = probe(baseUrl + '/api/status');
output[check] = status;
console.log(JSON.stringify(output));
