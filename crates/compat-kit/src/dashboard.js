async function post(path, data) {
  const response = await fetch(path, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Compat-Token': document.querySelector('meta[name="compat-token"]')?.content ?? '',
    },
    body: JSON.stringify(data),
  });
  const value = await response.json();
  document.getElementById('action').textContent = JSON.stringify(value);
  return value;
}

function runFixture() {
  const data = {
    fixture: document.getElementById('fixture').value,
    desktop: document.getElementById('desktop').checked,
  };
  const profile = document.getElementById('profile').value;
  if (profile) data.profile = profile;
  post('/api/run', data);
}

function showTask(task) {
  const paragraph = document.createElement('p');
  paragraph.textContent = task.fixture + ' / ' + task.profile + ' / ' + task.id + ' ' + task.kind + ' ' +
    (task.done ? JSON.stringify(task.result) : 'running');
  if (!task.done) {
    const button = document.createElement('button');
    button.textContent = 'Stop';
    button.onclick = () => post('/api/stop', {id: task.id});
    paragraph.appendChild(button);
  }
  return paragraph;
}

let fixtures = [];
function selectProfiles() {
  const fixture = fixtures.find(f => f.name === document.getElementById('fixture').value);
  const select = document.getElementById('profile');
  const previous = select.value;
  select.replaceChildren(...(fixture?.profiles ?? []).map(profile => {
    const option = document.createElement('option');
    option.value = profile;
    const version = profile.includes('6000.3') ? 'Unity 6.3 (6000.3)' :
      profile.includes('2017.1') ? 'Legacy Unity 2017.1' : profile;
    option.textContent = profile + ' / ' + version;
    return option;
  }));
  select.value = fixture?.profiles.includes(previous) ? previous : fixture?.default_profile ?? '';
}
document.getElementById('fixture').addEventListener('change', selectProfiles);
function filterResults() {
  const fields = ['case', 'game', 'profile', 'platform', 'status'];
  const view = document.getElementById('attempt-filter').value;
  for (const row of document.querySelectorAll('[data-view]')) {
    row.hidden = fields.some(field => {
      const selected = document.getElementById(field + '-filter').value;
      return selected && row.dataset[field] !== selected;
    }) || Boolean(view && row.dataset.view !== view);
  }
}
for (const field of ['case', 'game', 'profile', 'platform', 'status', 'attempt']) {
  document.getElementById(field + '-filter').addEventListener('change', filterResults);
}

let polling = false;
async function poll() {
  if (polling) return;
  polling = true;
  try {
    const response = await fetch('/api/state');
    if (!response.ok) return;
    const value = await response.json();
    fixtures = value.fixtures;
    selectProfiles();
    document.getElementById('tasks').replaceChildren(...value.tasks.map(showTask));
    const key = JSON.stringify(value.runs.map(run => run.run_id));
    const previous = sessionStorage.getItem('compat-runs');
    sessionStorage.setItem('compat-runs', key);
    if (previous && previous !== key) location.reload();
  } catch (error) {
    document.getElementById('action').textContent =
      'Live controls require compat-kit serve; ' + error;
  } finally {
    polling = false;
  }
}

if (location.protocol === 'http:') {
  poll();
  setInterval(poll, 1000);
} else {
  document.getElementById('action').textContent =
    'Static report. Start compat-kit serve for Run and Stop controls.';
}
