"""Validate actual replay state, session-local contact identities and GPU pixels."""
import argparse
import hashlib
import json
from pathlib import Path
from driver import ROOT, active_object, component, reset_state


def verify_wait(goal, near, launched, delay):
    assert reset_state(active_object(goal, 'Ball')), 'goal did not fully reset/hide/sleep ball'
    assert reset_state(active_object(near, 'Ball')), 'ball relaunched before the source deadline'
    before = near['time']['time'] - goal['time']['time']
    elapsed = launched['time']['time'] - goal['time']['time']
    assert delay - 0.11 <= before < delay, ('invalid pre-deadline native time', before)
    assert delay - 0.041 <= elapsed <= delay + 0.041, ('invalid launch native time', elapsed)
    ball = active_object(launched, 'Ball')
    body = component(ball, 'Rigidbody2D')['fields']
    assert component(ball, 'SpriteRenderer')['enabled']
    assert not body['sleeping']
    assert all(abs(v) == 12 for v in body['velocity'])
    return dict(before_deadline=before, wait_observed=elapsed, configured_delay=delay)


def contact(events, snapshot, other, trigger):
    handles = {active_object(snapshot, 'Ball')['handle'], active_object(snapshot, other)['handle']}
    matches = [e for e in events if e['phase'] == 'enter' and e['trigger'] == trigger
               and {e['a'], e['b']} == handles]
    assert matches, f'no actual Ball/{other} enter event'
    return matches[-1]


def verify_record(record_path, fixture_path):
    record = json.loads(record_path.read_text())
    fixture = json.loads(fixture_path.read_text())
    assert record['status'] == 'pass', record['messages']
    assert record['worker']['teardown'] == dict(completed=True, native_worlds=0, managed_roots=0, pending_readbacks=0)
    assert len(record['assertions']) == len(fixture['assertions']) == 366
    assert all(a['passed'] for a in record['assertions'])
    assert record['evidence_label'] == fixture['evidence_label'] == 'original-source/legacy-mappings-dejavu-font'
    observations = record['worker']['observations']
    checkpoints = fixture['metadata']['journey_checkpoints']
    def obs(label, selector='world.snapshot'):
        frame = next(c['frame'] for c in checkpoints if c['label'] == label)
        return next(o['value'] for o in observations if o['frame'] == frame and o['selector'] == selector)
    def scores(s):
        return [component(active_object(s, n), 'Text')['fields']['text']
                for n in ('PlayerOneScore', 'PlayerTwoScore')]
    assert scores(obs('initial')) == ['0', '0']
    assert scores(obs('round-1-goal-reset')) == ['0', '1']
    assert scores(obs('round-2-goal-reset')) == ['1', '1']
    final_scores = list(map(int, scores(obs('round-3-goal-reset'))))
    assert final_scores == [2, 1]
    checks = []
    for i in range(1, 4):
        label = f'round-{i}'
        bounce_label = next(c['label'] for c in checkpoints if c['label'].startswith(label + '-') and c['label'].endswith('-bounce'))
        side = 'right' if '-right-' in bounce_label else 'left'
        paddle = 'Enemy' if side == 'right' else 'Player'
        bounce = obs(f'{label}-{side}-bounce')
        physical = contact(obs(f'{label}-{side}-bounce', 'events'), bounce, paddle, False)
        vx = component(active_object(bounce, 'Ball'), 'Rigidbody2D')['fields']['velocity'][0]
        assert vx < 0 if side == 'right' else vx > 0
        goal = obs(f'{label}-goal-reset')
        goal_name = 'PlayerOneGoal' if side == 'right' else 'PlayerTwoGoal'
        scored = contact(obs(f'{label}-goal-reset', 'events'), goal, goal_name, True)
        goal_fields = component(active_object(goal, goal_name), 'Script')['fields']
        assert goal_fields['scoresTo'] == (1 if side == 'right' else 0)
        calls = goal_fields['scoreEvent']['m_PersistentCalls']['m_Calls']
        target = component(active_object(goal, 'MatchManager'), 'Script')['handle']
        assert any(c['m_MethodName'] == 'ScorePoint' and c['m_Target']['$ref'] == target for c in calls)
        delay = component(active_object(goal, 'MatchManager'), 'Script')['fields']['timeBeforeLaunch']
        assert scores(obs(f'{label}-before-deadline')) == scores(goal)
        assert scores(obs(f'{label}-relaunched')) == scores(goal)
        wait = verify_wait(goal, obs(f'{label}-before-deadline'), obs(f'{label}-relaunched'), delay)
        checks.append(dict(round=i, paddle=paddle, paddle_contact=physical, goal=goal_name, goal_contact=scored,
                           score=scores(goal), **wait))
    verify_controls(obs, scores)
    run_dir = ROOT / record['run_dir']
    request = json.loads((run_dir / 'request.json').read_text())
    assert request['timeline'] == fixture['timeline']
    assert request['profile'] == record['profile'] and request['seed'] == 17
    changes = verify_captures(record, fixture, run_dir)
    return dict(status='pass', run_id=record['run_id'], profile=record['profile'],
                assertions=len(record['assertions']), desktop=request['desktop'],
                rounds=checks, reload_renewed_control=True, teardown=record['worker']['teardown'],
                gpu_score_regions=changes, captures=record['captures'])


def verify_controls(obs, scores):
    for name, direction in [('Player', 1), ('Enemy', -1)]:
        def y(label):
            return active_object(obs(label), name)['transform']['position'][1]
        assert (y('controls-forward') - y('initial')) * direction > .79
        assert abs(y('controls-reverse') - y('initial')) < .001
        assert (y('reload-controls') - y('reloaded')) * direction > .79
    assert scores(obs('reloaded')) == ['0', '0']
    assert active_object(obs('reloaded'), 'Ball')['transform']['position'] == [0, 0, 0]
    assert obs('reloaded', 'events') == []


def verify_captures(record, fixture, run_dir):
    from PIL import Image, ImageChops
    expected_captures = [dict(path=a['path'], frame=a['frame'], phase=a['phase'],
                              origin=fixture['evidence_label'])
                         for a in fixture['timeline'] if a['action'] == 'capture']
    assert record['worker']['captures'] == expected_captures
    assert len(record['captures']) == len(expected_captures) == 5
    events = [json.loads(line) for line in (run_dir / 'player-events.ndjson').read_text().splitlines()]
    for expected, capture in zip(expected_captures, record['captures']):
        assert all(capture[key] == value for key, value in expected.items())
        assert capture['width'] == 960 and capture['height'] == 720
        assert any(e['action'] == 'capture' and e['stage'] == 'complete' and e['error'] is None
                   and e['frame'] == expected['frame'] and e['phase'] == expected['phase'] for e in events)
    for capture in record['captures']:
        assert hashlib.sha256((run_dir / capture['path']).read_bytes()).hexdigest() == capture['sha256']
    initial = Image.open(run_dir / 'captures/initial.png').convert('RGBA')
    both = Image.open(run_dir / 'captures/round-2-goal-reset.png').convert('RGBA')
    # Native authored anchors place the two scores at viewport quarter/three-quarter width.
    changes = []
    for center in (initial.width // 4, initial.width * 3 // 4):
        box = (center - 35, 20, center + 35, 90)
        diff = ImageChops.difference(initial.crop(box).convert('RGB'), both.crop(box).convert('RGB'))
        changed = sum(any(p) for p in diff.getdata())
        assert changed > 10, ('score pixels did not change', center)
        changes.append(dict(rect=list(box), changed_pixels=changed))
    return changes


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('record', type=Path)
    parser.add_argument('--fixture', type=Path, default=ROOT / 'fixtures/pong-journey.json')
    args = parser.parse_args()
    print(json.dumps(verify_record(args.record, args.fixture), indent=2))


if __name__ == '__main__':
    main()
