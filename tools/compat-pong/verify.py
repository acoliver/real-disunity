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


def verify_runtime(runtime, images):
    for name in ('player', 'harness'):
        assert runtime['before'][name] == images[name]
        assert runtime['after'][name] == images[name], ('executable changed during replay', name)
    assert runtime['workers'] and runtime['exit_code'] == 0
    native_sha = images.get('native', {}).get('sha256', runtime['workers'][0]['native_sha256'])
    assert native_sha
    for worker in runtime['workers']:
        assert worker['native_mapped'] and worker['native_map_lines']
        assert worker['player_sha256'] == images['player']['sha256']
        assert worker['native_sha256'] == native_sha


def validate_native_image_proof(proof_path, run_dir):
    proof = json.loads(proof_path.read_text())
    image = Path(proof['image'])
    work_root = (ROOT / '.work').resolve()
    resolved_image = image.resolve(strict=True)
    assert image.is_file() and not image.is_symlink(), 'native image must be a regular non-symlink file'
    assert resolved_image.is_relative_to(work_root), 'native image must remain inside this worktree .work'
    scope = proof.get('scope')
    if scope == 'process':
        assert isinstance(proof.get('process_id'), int) and proof['process_id'] > 0
    else:
        assert scope is None, ('unknown native image scope', scope)
        assert resolved_image.is_relative_to((run_dir / 'build').resolve()), 'legacy image must be run-local'
    assert image.stat().st_size == proof['bytes']
    assert hashlib.sha256(image.read_bytes()).hexdigest() == proof['sha256']
    return proof


def verify_record(record_path, fixture_path):
    from PIL import Image, ImageChops
    record = json.loads(record_path.read_text())
    fixture = json.loads(fixture_path.read_text())
    assert record['status'] == 'pass', record['messages']
    assert record['worker']['teardown'] == dict(completed=True, native_worlds=0, managed_roots=0, pending_readbacks=0)
    source = record['source_before']['scripts']
    assert source == record['source_after']['scripts']
    originals = {p: h for p, h in source.items() if p.startswith('Assets/Scripts/')}
    assert len(source) == 31
    assert len(originals) == 20 and originals == fixture['metadata']['source_sha256']
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
    for name, direction in [('Player', 1), ('Enemy', -1)]:
        def y(label):
            return active_object(obs(label), name)['transform']['position'][1]
        assert (y('controls-forward') - y('initial')) * direction > .79
        assert abs(y('controls-reverse') - y('initial')) < .001
        assert (y('reload-controls') - y('reloaded')) * direction > .79
    assert scores(obs('reloaded')) == ['0', '0']
    assert active_object(obs('reloaded'), 'Ball')['transform']['position'] == [0, 0, 0]
    assert obs('reloaded', 'events') == []
    run_dir = ROOT / record['run_dir']
    request = json.loads((run_dir / 'request.json').read_text())
    timeline_hash = hashlib.sha256(json.dumps(request['timeline'], sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    assert timeline_hash == fixture['metadata']['timeline_sha256']
    assert request['profile'] == record['profile'] and request['seed'] == 17
    for field in ('assembly', 'assembly_qualification', 'assembly_sha256'):
        assert field not in request['metadata'], ('final replay must compile fresh', field)
    proof_path = run_dir / 'build/original-source-count.json'
    proof = json.loads(proof_path.read_text())
    assert proof['source_count'] == proof['count'] == 20
    assert proof['source_hashes'] == originals
    assert set(proof['files']) == set(originals)
    assert proof['source_root'] == request['source_root']
    expected_inputs = sorted(str(Path(request['source_root']) / p) for p in originals)
    assert proof['compiler_inputs_verified'] is True
    assert sorted(proof['compiler_inputs']) == expected_inputs
    actual_inputs = (run_dir / 'build/actual-compile-items.txt').read_text(encoding='utf-8-sig').splitlines()
    assert sorted(actual_inputs) == expected_inputs
    assembly = Path(proof['assembly'])
    assert assembly.is_relative_to((run_dir / 'build').resolve())
    assert hashlib.sha256(assembly.read_bytes()).hexdigest() == proof['assembly_sha256']
    assert proof['metadata']['assembly'] == 'OriginalGame' and proof['metadata']['types']
    assert 'UnityCompat' in proof['metadata']['references']
    assert json.loads((run_dir / 'build/metadata-inspection.log').read_text()) == proof['metadata']
    native_proof_path = run_dir / 'build/native-image.json'
    native_proof = validate_native_image_proof(native_proof_path, run_dir)
    managed_images = {}
    for name in ('CompatibilityBridge.dll', 'UnityCompat.dll'):
        path = run_dir / 'build/bridge/bin/CompatibilityBridge/debug' / name
        managed_images[name] = dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
    expected_captures = [dict(path=a['path'], frame=a['frame'], phase=a['phase'],
                              origin=fixture['evidence_label'])
                         for a in fixture['timeline'] if a['action'] == 'capture']
    assert record['worker']['captures'] == expected_captures
    assert len(record['captures']) == len(expected_captures) == 5
    events = [json.loads(line) for line in (run_dir / 'player-events.ndjson').read_text().splitlines()]
    for receipt, capture in zip(expected_captures, record['captures']):
        assert all(capture[key] == value for key, value in receipt.items())
        assert capture['width'] == 960 and capture['height'] == 720
        assert any(e['action'] == 'capture' and e['stage'] == 'complete' and e['error'] is None
                   and e['frame'] == receipt['frame'] and e['phase'] == receipt['phase'] for e in events)
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
    return dict(status='pass', run_id=record['run_id'], profile=record['profile'],
                original_script_count=len(originals), total_script_count=len(source), original_scripts_unchanged=True,
                source_proof=str(proof_path.relative_to(ROOT)), assembly=proof,
                native_image_proof=native_proof, managed_images=managed_images,
                canonical_capture_receipts=expected_captures, assertions=len(record['assertions']),
                desktop=request['desktop'], timeline_sha256=timeline_hash,
                rounds=checks, reload_renewed_control=True, teardown=record['worker']['teardown'],
                gpu_score_regions=changes, captures=record['captures'])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('record', type=Path)
    parser.add_argument('--fixture', type=Path, default=ROOT / 'fixtures/pong-journey.json')
    args = parser.parse_args()
    print(json.dumps(verify_record(args.record, args.fixture), indent=2))


if __name__ == '__main__':
    main()
