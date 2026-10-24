"""Keyboard-only planning using canonical native observations."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / '.work/breakout-journey'


def compact_presentation(fixture, ticks_per_frame=10):
    from itertools import groupby
    if ticks_per_frame < 1:
        raise ValueError('presentation batch must contain at least one logical tick')
    compact = copy.deepcopy(fixture)
    mapping, presented, count, captured = {}, 0, 0, False
    for tick, group in groupby(compact['timeline'], key=lambda a: a['frame']):
        actions = list(group)
        if captured or count >= ticks_per_frame:
            presented += 1
            count = 0
        mapping[tick] = presented
        for item in actions:
            item['frame'] = presented
        captured = any(a['action'] == 'capture' for a in actions)
        count += 1
    for assertion in compact['assertions']:
        if assertion.get('frame') is not None:
            assertion['frame'] = mapping[assertion['frame']]
    for checkpoint in compact['metadata']['journey_checkpoints']:
        checkpoint['logical_frame'] = checkpoint['frame']
        checkpoint['frame'] = mapping[checkpoint['frame']]
    compact['metadata']['presentation'] = dict(logical_to_presented=mapping,
        maximum_logical_ticks_per_presentation=ticks_per_frame,
        native_advances='unchanged ordered .01-second advances; presentation labels only')
    return compact


def replay_fixture(fixture, timeline, frame, case, evidence, sample_frames=()):
    from itertools import groupby
    requested = set(sample_frames)
    advanced = {a['frame'] for a in timeline if a['action'] == 'advance'}
    if requested - advanced:
        raise ValueError('samples require an actual advanced frame')
    sampled = []
    for tick, group in groupby(timeline, key=lambda a: a['frame']):
        sampled.extend(group)
        if tick in requested:
            sampled.extend([action(tick, 'observe', 'sample', selector='world.snapshot'),
                            action(tick, 'observe', 'sample', selector='events')])
    timeline = sampled
    allowed = {'load', 'input', 'advance', 'observe', 'capture', 'reload', 'unload'}
    for item in timeline:
        verb = item['action']
        if verb not in allowed:
            raise ValueError('only public keyboard playback actions are permitted')
        if verb == 'advance' and not 0 < item['dt'] <= 1 / 60:
            raise ValueError('every native advance must be bounded to 1/60 second')
        if verb == 'input' and (set(item.get('keys', [])) - {'A', 'D'} or item.get('axes')):
            raise ValueError('Breakout uses only keyboard A/D, without injected axes')
        if verb == 'observe' and item['selector'] not in {'world.snapshot', 'events'}:
            raise ValueError('planner observations must be public native state')
    if sum(a['action'] == 'capture' for a in timeline) > 32:
        raise ValueError('capture budget exceeded')
    frozen = copy.deepcopy(fixture)
    frozen.update(name='BreakoutJourney-' + case, refresh=None,
                  profiles=['compat-2017.1', 'compat-6000.3'], default_profile='compat-6000.3',
                  timeline=copy.deepcopy(timeline) + [action(frame, 'unload')], assertions=[],
                  limits=dict(deadline_ms=120000, log_bytes=1048576,
                              artifact_bytes=268435456, capture_count=32))
    for key in ['assembly', 'assembly_qualification']:
        frozen['metadata'].pop(key, None)
    frozen['metadata']['coverage'] = evidence
    return frozen


def tracking_keys(paddle_x, balls, offset):
    if not balls:
        return []
    descending = [b for b in balls if b[2] < 0]
    selected = min(descending or balls, key=lambda b: b[1])
    return steer(paddle_x, selected[0] - offset)


def live(snapshot, name):
    return [o for o in snapshot['objects'] if o['active_in_hierarchy']
            and o['name'].removesuffix('(Clone)') == name]


def action(frame, verb, phase=None, **fields):
    return dict(frame=frame, phase=phase or verb, action=verb, **fields)


def steps(frame, keys):
    return [action(frame, 'input', keys=list(keys), axes={}),
            action(frame, 'advance', 'fixed_update', dt=.01),
            action(frame, 'advance', 'update', dt=.01)]


def steer(current, target):
    if abs(target - current) <= .11:
        return []
    return ['D' if target > current else 'A']


def source_inventory(source):
    files = sorted(source.glob('Assets/Scripts/**/*.cs'))
    expected = {'Ball', 'BallSpawner', 'Brick', 'BrickSpawner', 'DeathZone', 'Platform', 'PowerUp'}
    if len(files) != 7 or {p.stem for p in files} != expected:
        raise ValueError('expected exactly the seven original Breakout sources')
    return {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}


def component(obj, kind):
    return next(c for c in obj['components'] if c['kind'] == kind)


def position(obj):
    return obj['transform']['position'][:2]


def velocity(obj):
    return component(obj, 'Rigidbody2D')['fields']['velocity']


def dump(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


class Planner:
    def __init__(self, resume=False, seed=17, case="main"):
        if not case.replace("-", "").isalnum():
            raise ValueError("case must be an alphanumeric directory name")
        self.work = WORK if case == "main" else WORK / case
        self.work.mkdir(parents=True, exist_ok=True)
        self.fixture = json.loads((ROOT / 'fixtures/breakout.json').read_text())
        self.fixture.update(name='BreakoutJourney', refresh=None, seed=seed)
        self.source = ROOT / self.fixture['source_root']
        self.before = source_inventory(self.source)
        self.timeline = [action(0, 'load')]
        self.frame, self.serial = 0, 0
        self.checkpoints, self.samples = [], []
        if resume:
            state = json.loads((self.work / 'resume.json').read_text())
            for key in ['timeline', 'frame', 'serial', 'checkpoints', 'snapshot', 'events']:
                setattr(self, key, state[key])
            self.fixture['metadata'] = state['metadata']
            self.fixture['seed'] = state['seed']
            assert self.before == state['source_hashes']

    def run(self, count=0, keys=(), every=5):
        if self.serial >= 100 or self.frame + count > 5000:
            raise RuntimeError('bounded planner exhausted: 100 prefixes / 5000 logical ticks')
        start = self.frame
        for index in range(count):
            self.frame += 1
            selected = keys(index) if callable(keys) else keys
            self.timeline.extend(steps(self.frame, selected))
            if self.frame % every == 0 or index == count - 1:
                self.timeline.append(action(self.frame, 'observe', 'sample', selector='world.snapshot'))
                self.timeline.append(action(self.frame, 'observe', 'sample', selector='events'))
        self.serial = max([self.serial] + [int(p.name.split('-')[1]) for p in self.work.glob('prefix-*')]) + 1
        folder = self.work / f'prefix-{self.serial:04}'
        folder.mkdir(exist_ok=True)
        timeline = [a for a in self.timeline if a['action'] != 'capture']
        timeline += [action(self.frame, 'observe', selector='world.snapshot'),
                     action(self.frame, 'observe', selector='events'), action(self.frame, 'unload')]
        request = dict(schema_version=1, run_id=folder.name, fixture='BreakoutJourney',
                       profile='compat-6000.3', desktop=False, seed=self.fixture['seed'], timeline=timeline,
                       assertions=[], source_root=str(self.source), source_hashes=self.before,
                       artifact_root=str(folder), metadata=self.fixture['metadata'])
        dump(folder / 'request.json', request)
        env = dict(os.environ, COMPAT_ROOT=str(ROOT), COMPAT_REQUEST=str(folder / 'request.json'),
                   COMPAT_RESPONSE=str(folder / 'response.json'), COMPAT_ARTIFACTS=str(folder))
        began = time.monotonic()
        with (folder / 'stdout.log').open('w') as out, (folder / 'stderr.log').open('w') as err:
            subprocess.run([str(ROOT / '.work/target/debug/compat-player')], cwd=ROOT, env=env,
                           stdout=out, stderr=err, timeout=120, check=True)
        report = json.loads((folder / 'response.json').read_text())
        if report['diagnostics']:
            raise RuntimeError(report['diagnostics'])
        assert report['teardown'] == dict(completed=True, native_worlds=0, managed_roots=0, pending_readbacks=0)
        assert source_inventory(self.source) == self.before
        self.snapshot, self.events = [o['value'] for o in report['observations'][-2:]]
        self.samples = [o for o in report['observations'] if o['phase'] == 'sample' and o['frame'] > start]
        self.timeline = [a for a in self.timeline if a['phase'] != 'sample']
        if 'assembly' not in self.fixture['metadata']:
            proof = folder / 'build/original-source-count.json'
            qualification = json.loads(proof.read_text())
            if 'assembly_sha256' in qualification:
                self.fixture['metadata'].update(assembly=qualification['assembly'], assembly_qualification=str(proof))
        self.save()
        print('prefix', self.serial, 'tick', self.frame, 'seconds', round(time.monotonic()-began, 2),
              'bricks', len(live(self.snapshot, 'Brick')), 'balls', len(live(self.snapshot, 'Ball')),
              'paddle', position(self.paddle()), 'width', self.width(), flush=True)
        return report

    def save(self):
        dump(self.work / 'resume.json', dict(timeline=self.timeline, frame=self.frame, serial=self.serial,
             checkpoints=self.checkpoints, snapshot=self.snapshot, events=self.events,
             metadata=self.fixture['metadata'], seed=self.fixture['seed'], source_hashes=self.before))

    def paddle(self):
        return live(self.snapshot, 'Platform')[0]

    def width(self):
        return component(self.paddle(), 'BoxCollider2D')['fields']['size'][0]

    def mark(self, label, capture=True):
        self.timeline += [action(self.frame, 'observe', label, selector='world.snapshot'),
                          action(self.frame, 'observe', label, selector='events')]
        if capture:
            self.timeline.append(action(self.frame, 'capture', 'render', path=f'captures/{label}.png'))
        self.checkpoints.append(dict(label=label, frame=self.frame, snapshot=copy.deepcopy(self.snapshot),
                                     events=copy.deepcopy(self.events)))
        self.save()
        print('MILESTONE', label, self.frame, flush=True)

    def fixed_target(self, count, target):
        initial = position(self.paddle())[0]
        ticks = round(abs(target-initial)/.2)
        return self.run(count, lambda i: steer(initial, target) if i < ticks else [])

    def follow(self, ticks, offset=.65):
        end = self.frame + ticks
        while self.frame < end:
            balls = [(position(b)[0], position(b)[1], velocity(b)[1])
                     for b in live(self.snapshot, 'Ball')]
            keys = tracking_keys(position(self.paddle())[0], balls, offset)
            near = any(y < -1.5 and vy < 0 for _, y, vy in balls)
            count = min(2 if near else 10, end - self.frame)
            old_balls = len(balls)
            old_width = self.width()
            old_events = len(self.events)
            self.run(count, keys, every=1)
            if len(live(self.snapshot, 'Ball')) != old_balls or self.width() != old_width:
                self.mark(f'source-transition-{self.frame}')
            if len(self.events) > old_events:
                self.mark(f'contact-{self.frame}', capture=False)

    def reload(self, label='reloaded'):
        self.frame += 1
        self.timeline.append(action(self.frame, 'reload'))
        self.run()
        self.mark(label)

    def controls(self):
        self.run(1)
        self.mark('initial')
        self.run(40, ['D'])
        self.mark('right-clamp')
        self.reload()
        self.run(40, ['A'])
        self.mark('left-clamp')
        self.reload('second-reload')
        self.run(2, ['D'])
        self.mark('renewed-control')

    def export(self, name, sample_frames=(), compact=False):
        fixture = replay_fixture(self.fixture, self.timeline, self.frame, name,
                                 dict(status='partial', checkpoints=[c['label'] for c in self.checkpoints],
                                      complete_journey=False), sample_frames=sample_frames)
        fixture['metadata']['source_sha256'] = self.before
        fixture['metadata']['journey_checkpoints'] = [dict(label=c['label'], frame=c['frame'],
            brick_count=len(live(c['snapshot'], 'Brick')), ball_count=len(live(c['snapshot'], 'Ball')))
            for c in self.checkpoints]
        for cp in self.checkpoints:
            for index, obj in enumerate(cp['snapshot']['objects']):
                if not obj['active_in_hierarchy'] or obj['name'] not in ['Platform', 'Ball', 'Brick']:
                    continue
                base = f'/objects/{index}'
                fixture['assertions'].append(dict(op='equal', selector='world.snapshot', phase=cp['label'],
                    frame=cp['frame'], pointer=base + '/name', value=obj['name']))
                if obj['name'] in ['Platform', 'Ball']:
                    for axis in range(2):
                        fixture['assertions'].append(dict(op='tolerance', selector='world.snapshot', phase=cp['label'],
                            frame=cp['frame'], pointer=base + f'/transform/position/{axis}',
                            value=position(obj)[axis], tolerance=.001))
                for ci, item in enumerate(obj['components']):
                    if item['kind'] in ['BoxCollider2D', 'SpriteRenderer']:
                        pointer = base + f'/components/{ci}'
                        fixture['assertions'].append(dict(op='equal', selector='world.snapshot', phase=cp['label'],
                            frame=cp['frame'], pointer=pointer + '/enabled', value=item['enabled']))
                        field = 'size' if item['kind'] == 'BoxCollider2D' else 'sprite'
                        fixture['assertions'].append(dict(op='equal', selector='world.snapshot', phase=cp['label'],
                            frame=cp['frame'], pointer=pointer + '/fields/' + field, value=item['fields'][field]))
        for item in self.timeline:
            if item['action'] == 'capture':
                fixture['assertions'].append(dict(op='capture', frame=item['frame'], phase=item['phase'],
                    origin=fixture['evidence_label']))
        if compact:
            fixture = compact_presentation(fixture)
        path = ROOT / f'fixtures/breakout-journey-{name}.json'
        dump(path, fixture)
        dump(self.work / 'planned-checkpoints.json', self.checkpoints)
        return path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--resume', action='store_true')
    parser.add_argument('--probe', type=int, default=1)
    parser.add_argument('--seed', type=int, default=17)
    parser.add_argument('--case', default='main')
    parser.add_argument('--follow', type=int, default=0)
    parser.add_argument('--offset', type=float, default=.65)
    parser.add_argument('--export', action='store_true')
    parser.add_argument('--compact', action='store_true', help='batch presentation labels without changing native stimuli')
    parser.add_argument('--scenario', choices=['probe', 'controls', 'powerup'], default='probe')
    args = parser.parse_args()
    planner = Planner(args.resume, args.seed, args.case)
    if args.scenario == 'controls':
        planner.controls()
    elif args.scenario == 'powerup':
        planner.run(1)
        planner.mark('initial')
        for ticks, label in [(13, 'brick-disabled'), (10, 'brick-animation'), (16, 'source-powerup')]:
            planner.run(ticks, every=1)
            planner.mark(label)
    else:
        planner.run(args.probe)
        planner.mark('initial' if not args.resume else f'probe-{planner.frame}')
    if args.follow:
        planner.follow(args.follow, args.offset)
    if args.export:
        print(planner.export(args.case, compact=args.compact))


if __name__ == '__main__':
    main()
