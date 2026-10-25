"""Plan keyboard-only Pong prefixes by executing the canonical compatibility worker."""
import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]

sys.path.insert(0, str(ROOT / "tools"))
from original_games import checkout
DT = 0.02


def active_object(snapshot, name):
    matches = [o for o in snapshot['objects'] if o['name'] == name and o['active_in_hierarchy']]
    if len(matches) != 1:
        raise ValueError(f'{name}: expected one live object, found {len(matches)}')
    return matches[0]


def component(obj, kind):
    return next(c for c in obj['components'] if c['kind'] == kind)


def steering_keys(current, target, positive, negative):
    if abs(target - current) <= 0.21:
        return []
    return [positive if target > current else negative]


def step_actions(frame, keys):
    return [dict(frame=frame, phase='input', action='input', keys=keys, axes={}),
            dict(frame=frame, phase='fixed_update', action='advance', dt=DT)]


def reset_state(ball):
    body = component(ball, 'Rigidbody2D')['fields']
    return (ball['transform']['position'] == [0, 0, 0]
            and body['velocity'] == [0, 0] and body['angular_velocity'] == 0
            and body['sleeping'] and not component(ball, 'SpriteRenderer')['enabled'])


def action(frame, verb, phase=None, **kwargs):
    return dict(frame=frame, phase=phase or verb, action=verb, **kwargs)


class Planner:
    def __init__(self, worker, source, output):
        self.worker, self.work = worker, output
        self.fixture = json.loads((ROOT / 'fixtures/pong.json').read_text())
        self.fixture['name'] = 'pong-journey'
        self.source = source
        self.timeline = [action(0, 'load')]
        self.frame = 0
        self.serial = 0
        self.checkpoints = []

    def run(self, count=0, keys=(), sample=False):
        old_scores = self.scores() if hasattr(self, "snapshot") else None
        sample_start = self.frame
        for _ in range(count):
            self.frame += 1
            self.timeline += step_actions(self.frame, list(keys) if not callable(keys) else keys(_))
            if sample:
                self.timeline.append(action(self.frame, "observe", "sample", selector="world.snapshot"))
        self.serial += 1
        folder = self.work / f'prefix-{self.serial:04}'
        folder.mkdir(parents=True, exist_ok=True)
        request = dict(schema_version=1, run_id=folder.name, fixture='pong-journey',
                       profile='compat-2017.1', desktop=False, seed=17,
                       timeline=[a for a in self.timeline if a['action'] != 'capture'] + [action(self.frame, 'observe', selector='world.snapshot'),
                           action(self.frame, 'observe', selector='events'), action(self.frame, 'unload')],
                       assertions=[], source_root=str(self.source),
                       artifact_root=str(folder), metadata=self.fixture['metadata'])
        (folder / 'request.json').write_text(json.dumps(request))
        env = dict(os.environ, COMPAT_ROOT=str(ROOT), COMPAT_REQUEST=str(folder / 'request.json'),
                   COMPAT_RESPONSE=str(folder / 'response.json'), COMPAT_ARTIFACTS=str(folder))
        with (folder / 'stdout.log').open('w') as out, (folder / 'stderr.log').open('w') as err:
            subprocess.run([str(self.worker)], cwd=ROOT,
                           env=env, stdout=out, stderr=err, timeout=180, check=True)
        report = json.loads((folder / 'response.json').read_text())
        if report['diagnostics']:
            raise RuntimeError(report['diagnostics'])
        assert report['teardown'] == dict(completed=True, native_worlds=0, managed_roots=0, pending_readbacks=0)
        self.snapshot = report['observations'][-2]['value']
        self.events = report['observations'][-1]['value']
        if sample:
            for observation in report['observations']:
                if observation['phase'] != 'sample' or observation['frame'] <= sample_start:
                    continue
                self.snapshot = observation['value']
                changed = (component(self.ball(), 'SpriteRenderer')['enabled'] if sample == 'launch'
                           else self.scores() != old_scores)
                if changed:
                    self.frame = observation['frame']
                    self.timeline = [a for a in self.timeline if a['frame'] <= self.frame]
                    break
            self.timeline = [a for a in self.timeline if a['phase'] != 'sample']
        self.save_state()
        return self.snapshot

    def save_state(self):
        (self.work / 'resume.json').write_text(json.dumps(dict(timeline=self.timeline, frame=self.frame,
            serial=self.serial, checkpoints=self.checkpoints, snapshot=self.snapshot, events=self.events)))

    def mark(self, label, capture=False):
        self.timeline.append(action(self.frame, 'observe', 'observe', selector='world.snapshot'))
        self.timeline.append(action(self.frame, 'observe', 'observe', selector='events'))
        if capture:
            self.timeline.append(action(self.frame, 'capture', 'render', path=f'captures/{label}.png'))
        self.checkpoints.append(dict(label=label, frame=self.frame, snapshot=copy.deepcopy(self.snapshot), events=copy.deepcopy(self.events)))
        self.save_state()
        print(label, self.frame, 'scores', self.scores(), 'ball', self.ball()['transform']['position'], flush=True)

    def ball(self):
        return active_object(self.snapshot, 'Ball')

    def scores(self):
        return [component(active_object(self.snapshot, n), 'Text')['fields']['text']
                for n in ('PlayerOneScore', 'PlayerTwoScore')]

    def controls(self):
        self.run()
        self.mark('initial', True)
        self.run(2, ['W', 'DownArrow'])
        self.mark('controls-forward')
        self.run(2, ['S', 'UpArrow'])
        self.mark('controls-reverse')

    def rally(self, side, label, bounced=False):
        """Track measured Y with one paddle; clear the opposite paddle for a goal."""
        start_scores = self.scores()
        name = 'Enemy' if side == 'right' else 'Player'
        target_sign = 1 if side == 'right' else -1
        limit = self.frame + 350
        while self.frame < limit:
            ball = self.ball()
            x, y, _ = ball['transform']['position']
            vx, vy = component(ball, 'Rigidbody2D')['fields']['velocity']
            keys = []
            for paddle, up, down in [('Player', 'W', 'S'), ('Enemy', 'UpArrow', 'DownArrow')]:
                obj = active_object(self.snapshot, paddle)
                target = y if not bounced else (-6.5 if y >= 0 else 6.5)
                keys += steering_keys(obj['transform']['position'][1], target, up, down)
            # Native per-frame samples locate goals inside bounded flight prefixes.
            steps = (110 if bounced else (1 if abs(x) > 8 else 6))
            old_events = len(self.events)
            self.steer_rally(bounced, (target_sign, x, y, vx), steps, keys)
            if not bounced:
                handles = {active_object(self.snapshot, name)['handle'], self.ball()['handle']}
                hit = any(not e['trigger'] and e['phase'] == 'enter' and {e['a'], e['b']} == handles
                          for e in self.events[old_events:])
                if hit:
                    new_vx = component(self.ball(), 'Rigidbody2D')['fields']['velocity'][0]
                    assert new_vx * target_sign < 0, ('contact did not reverse velocity', new_vx)
                    bounced = True
                    self.mark(f'{label}-{side}-bounce')
            if self.scores() != start_scores:
                assert bounced, ('goal before desired paddle bounce', side, self.scores())
                expected = list(map(int, start_scores))
                expected[1 if side == 'right' else 0] += 1
                assert self.scores() == list(map(str, expected))
                assert reset_state(self.ball()), self.ball()
                self.mark(f'{label}-goal-reset', True)
                return
        raise RuntimeError(f'no goal: {side} at {self.frame}')

    def steer_rally(self, bounced, motion, steps, keys):
        target_sign, x, y, vx = motion
        if (not bounced and vx * target_sign > 0 and abs(x) < 8
                and any(c['label'] == 'round-1-right-bounce' for c in self.checkpoints)):
            # Reuse an empirically observed interception target for this symmetric source arena.
            # Only control planning uses this target; every resulting state/contact is read natively.
            first = next(c for c in self.checkpoints if c['label'] == 'round-1-right-bounce')
            serve = next((c['snapshot'] for c in reversed(self.checkpoints)
                          if c['label'].endswith('-relaunched')), self.snapshot)
            launch_y = component(active_object(serve, 'Ball'), 'Rigidbody2D')['fields']['velocity'][1]
            target_y = abs(active_object(first['snapshot'], 'Ball')['transform']['position'][1]) * (1 if launch_y >= 0 else -1)
            starts = {p: active_object(self.snapshot, p)['transform']['position'][1] for p in ['Player', 'Enemy']}
            travel = max(1, round((10 - .32 - abs(x)) / abs(vx) / DT) + 1)
            def intercept(index):
                keys = []
                for p, up, down in [('Player', 'W', 'S'), ('Enemy', 'UpArrow', 'DownArrow')]:
                    delta = target_y - starts[p]
                    if index < round(abs(delta) / (20 * DT)):
                        keys.append(up if delta > 0 else down)
                return keys
            self.run(travel, intercept)
        elif bounced:
            positions = {p: active_object(self.snapshot, p)['transform']['position'][1]
                         for p in ['Player', 'Enemy']}
            def parked(index):
                result = []
                target = -6.5 if y >= 0 else 6.5
                for p, up, down in [('Player', 'W', 'S'), ('Enemy', 'UpArrow', 'DownArrow')]:
                    distance = target - positions[p]
                    if index < round(abs(distance) / (20 * DT)):
                        result.append(up if distance > 0 else down)
                return result
            self.run(steps, parked, sample=True)
        else:
            self.run(steps, keys)

    def wait_launch(self, label):
        assert reset_state(self.ball())
        goal = next(c['snapshot'] for c in self.checkpoints if c['label'] == f'{label}-goal-reset')
        wait = component(active_object(self.snapshot, 'MatchManager'), 'Script')['fields']['timeBeforeLaunch']
        elapsed = self.snapshot['time']['time'] - goal['time']['time']
        remaining = max(0, round((wait - elapsed) / DT) - 2)
        if remaining:
            self.run(remaining, [])
        assert reset_state(self.ball()), 'early relaunch'
        self.mark(f'{label}-before-deadline')
        self.run(5, [], sample='launch')
        assert component(self.ball(), 'SpriteRenderer')['enabled'], 'launch coroutine did not resume'
        assert not component(self.ball(), 'Rigidbody2D')['fields']['sleeping']
        assert all(abs(v) == 12 for v in component(self.ball(), 'Rigidbody2D')['fields']['velocity'])
        self.mark(f'{label}-relaunched')

    def append_wait_contract(self, label):
        self.tail_checkpoints = []
        self.tail_assertions = []
        goal = next(c['snapshot'] for c in self.checkpoints if c['label'] == f'{label}-goal-reset')
        ball = active_object(goal, 'Ball')
        base = f"/objects/{goal['objects'].index(ball)}/components/"
        body = ball['components'].index(component(ball, 'Rigidbody2D'))
        sprite = ball['components'].index(component(ball, 'SpriteRenderer'))
        for count, suffix, visible in [(98, 'before-deadline', False), (3, 'relaunched', True)]:
            for _ in range(count):
                self.frame += 1
                self.timeline += step_actions(self.frame, [])
            self.timeline += [action(self.frame, 'observe', selector='world.snapshot'),
                              action(self.frame, 'observe', selector='events')]
            self.tail_checkpoints.append(dict(label=f'{label}-{suffix}', frame=self.frame))
            for pointer, value in [(f'{base}{sprite}/enabled', visible),
                                   (f'{base}{body}/fields/sleeping', not visible),
                                   (f'{base}{body}/fields/angular_velocity', 0.0)]:
                self.tail_assertions.append(dict(op='equal', selector='world.snapshot', phase='observe',
                                                 frame=self.frame, pointer=pointer, value=value))
            if not visible:
                self.tail_assertions.append(dict(op='equal', selector='world.snapshot', phase='observe',
                    frame=self.frame, pointer=f'{base}{body}/fields/velocity', value=[0.0, 0.0]))

    def append_reload_contract(self):
        self.frame += 1
        reload_frame = self.frame
        self.timeline += [action(self.frame, 'reload'),
                          action(self.frame, 'observe', selector='world.snapshot'),
                          action(self.frame, 'observe', selector='events'),
                          action(self.frame, 'capture', 'render', path='captures/reloaded.png')]
        for _ in range(2):
            self.frame += 1
            self.timeline += step_actions(self.frame, ['W', 'DownArrow'])
        self.timeline += [action(self.frame, 'observe', selector='world.snapshot'),
                          action(self.frame, 'observe', selector='events')]
        self.tail_checkpoints = getattr(self, 'tail_checkpoints', []) + [dict(label='reloaded', frame=reload_frame),
                                 dict(label='reload-controls', frame=self.frame)]
        self.tail_assertions = getattr(self, 'tail_assertions', []) + [dict(op='capture', frame=reload_frame, phase='render', origin=self.fixture['evidence_label'])]
        initial = next(c['snapshot'] for c in self.checkpoints if c['label'] == 'initial')
        for name in ['PlayerOneScore', 'PlayerTwoScore', 'Ball', 'Player', 'Enemy']:
            obj = active_object(initial, name)
            base = f"/objects/{initial['objects'].index(obj)}"
            for frame in [reload_frame, self.frame]:
                def equal(pointer, value):
                    self.tail_assertions.append(dict(op='equal', selector='world.snapshot', phase='observe',
                                                     frame=frame, pointer=base + pointer, value=value))
                equal('/name', name)
                equal('/active_in_hierarchy', True)
                if name.endswith('Score'):
                    j = obj['components'].index(component(obj, 'Text'))
                    equal(f'/components/{j}/fields/text', '0')
                elif frame == reload_frame:
                    equal('/transform/position', [0.0, 0.0, 0.0] if name == 'Ball' else [-10.0 if name == 'Player' else 10.0, 0.0, 0.0])
                elif name in ['Player', 'Enemy']:
                    self.tail_assertions.append(dict(op='tolerance', selector='world.snapshot', phase='observe',
                        frame=frame, pointer=base + '/transform/position/1', value=.8 if name == 'Player' else -.8, tolerance=.0001))
        self.tail_assertions.append(dict(op='equal', selector='events', frame=reload_frame, phase='observe', pointer='', value=[]))

    def finish(self):
        self.fixture['metadata']['journey_checkpoints'] = [dict(label=c['label'], frame=c['frame']) for c in self.checkpoints] + getattr(self, 'tail_checkpoints', [])
        self.fixture['limits']['deadline_ms'] = 300000
        self.fixture['timeline'] = self.timeline + [action(self.frame + 1, 'unload')]
        self.fixture['assertions'] = assertions(self.checkpoints, self.fixture['evidence_label']) + getattr(self, 'tail_assertions', [])
        output = self.work / 'pong-journey.json'
        output.write_text(json.dumps(self.fixture, indent=2) + '\n')
        (self.work / 'planned-checkpoints.json').write_text(json.dumps(self.checkpoints))
        print('FROZEN', output, len(self.timeline), 'actions', len(self.fixture['assertions']), 'assertions', flush=True)


def assertions(checkpoints, origin):
    result = []
    def equal(frame, pointer, value):
        result.append(dict(op='equal', selector='world.snapshot', pointer=pointer, value=value, frame=frame, phase='observe'))
    for cp in checkpoints:
        snapshot, frame, label = cp['snapshot'], cp['frame'], cp['label']
        for name in ['PlayerOneScore', 'PlayerTwoScore', 'Ball', 'Player', 'Enemy']:
            obj = active_object(snapshot, name)
            i = snapshot['objects'].index(obj)
            base = f'/objects/{i}'
            equal(frame, base + '/name', name)
            equal(frame, base + '/active_in_hierarchy', True)
            if name.endswith('Score'):
                j = obj['components'].index(component(obj, 'Text'))
                equal(frame, f'{base}/components/{j}/fields/text', component(obj, 'Text')['fields']['text'])
            else:
                for axis in (0, 1):
                    result.append(dict(op='tolerance', selector='world.snapshot', pointer=f'{base}/transform/position/{axis}',
                                       value=obj['transform']['position'][axis], tolerance=0.0001, frame=frame, phase='observe'))
            if name == 'Ball':
                for kind, fields in [('Rigidbody2D', ['velocity', 'angular_velocity', 'sleeping']), ('SpriteRenderer', [])]:
                    c = component(obj, kind)
                    j = obj['components'].index(c)
                    equal(frame, f'{base}/components/{j}/enabled', c['enabled'])
                    for field in fields:
                        equal(frame, f'{base}/components/{j}/fields/{field}', c['fields'][field])
        if label == 'initial' or label.endswith('goal-reset') or label == 'reloaded':
            result.append(dict(op='capture', frame=frame, phase='render', origin=origin))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--resume', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    planner = Planner(args.worker.resolve(), checkout("pong"), args.output.resolve())
    if args.resume:
        state = json.loads((args.output / 'resume.json').read_text())
        for key, value in state.items():
            setattr(planner, key, value)
    else:
        planner.controls()
    for i, side in enumerate(['right', 'left', 'right']):
        if i == 2:
            existing = next((c['label'] for c in planner.checkpoints if c['label'].startswith('round-3-') and c['label'].endswith('-bounce')), None)
            side = ('left' if '-left-' in existing else 'right') if existing else ('right' if component(planner.ball(), 'Rigidbody2D')['fields']['velocity'][0] > 0 else 'left')
        label = f'round-{i + 1}'
        if any(c['label'] == f'{label}-relaunched' for c in planner.checkpoints):
            continue
        if not any(c['label'] == f'{label}-goal-reset' for c in planner.checkpoints):
            planner.rally(side, label, any(c['label'] == f'{label}-{side}-bounce' for c in planner.checkpoints))
        if i == 2:
            planner.append_wait_contract(label)
        else:
            planner.wait_launch(label)
    planner.append_reload_contract()
    planner.finish()


if __name__ == '__main__':
    main()
