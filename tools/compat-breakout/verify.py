"""Summarize observations from real player execution without supplying game state."""
import json
import math
from pathlib import Path
from driver import ROOT, component, live, position, velocity


def clamps_reach_edges(widths, width, limit=8):
    edge = limit - width / 2
    return all(any(row['width'] == width and abs(row['x'] - sign * edge) <= .001
                   for row in widths) for sign in [-1, 1])


def width_evidence(report, duration=8):
    result = dict(expired=False, observed_past_initial_duration=False, intervals=[])
    active, previous_time = None, -1
    for row in report['observations']:
        if row['selector'] != 'world.snapshot':
            continue
        snapshot = row['value']
        now = snapshot['time']['time']
        if now < previous_time:
            active = None
        previous_time = now
        paddle = live(snapshot, 'Platform')[0]
        width = component(paddle, 'BoxCollider2D')['fields']['size'][0]
        if width == 4:
            if active is None:
                active = dict(start_frame=row['frame'], start_time=now)
                result['intervals'].append(active)
            active.update(last_wide_frame=row['frame'], last_wide_time=now)
            if now - active['start_time'] > duration + .02:
                result['observed_past_initial_duration'] = True
        elif width == 2 and active is not None:
            active.update(normal_frame=row['frame'], normal_time=now)
            result['expired'] = True
            active = None
    return result


def loss_evidence(report):
    result = dict(no_premature_respawn=False, last_loss_fresh_launch=False)
    pair, survivor, replacement, previous_time = None, None, None, -1
    for row in report['observations']:
        if row['selector'] != 'world.snapshot':
            continue
        snapshot = row['value']
        now = snapshot['time']['time']
        if now < previous_time:
            pair, survivor, replacement = None, None, None
        previous_time = now
        current = {b['handle']: b for b in live(snapshot, 'Ball')}
        ids = set(current)
        if pair is None and survivor is None and len(ids) == 2:
            pair = ids
        if pair is not None and pair - ids:
            if len(ids) != 1 or not ids < pair:
                return result
            survivor = next(iter(ids))
            result.update(no_premature_respawn=True, survivor=survivor,
                          first_loss_frame=row['frame'], first_loss_time=now,
                          removed=sorted(pair - ids))
            pair = None
        if survivor is not None and survivor not in ids and replacement is None:
            if len(ids) != 1:
                return result
            replacement = next(iter(ids))
            result.update(replacement=replacement, last_loss_frame=row['frame'], last_loss_time=now)
        if replacement in current:
            ball = current[replacement]
            renderer = component(ball, 'SpriteRenderer')
            if velocity(ball)[1] > 0 and renderer['enabled']:
                result.update(last_loss_fresh_launch=True, launch_frame=row['frame'], launch_time=now)
                return result
    return result


def summarize(report):
    rows = [o for o in report['observations'] if o['selector'] == 'world.snapshot']
    names, animations, disabled, removed = {}, {}, set(), set()
    previous_bricks = set()
    balls, widths, speeds, times = [], [], [], []
    for row in rows:
        snapshot = row['value']
        for obj in snapshot['objects']:
            names[obj['handle']] = obj['name']
        bricks = live(snapshot, 'Brick')
        ids = {o['handle'] for o in bricks}
        if not times or snapshot['time']['time'] >= times[-1]:
            removed.update(previous_bricks - ids)
        previous_bricks = ids
        for brick in bricks:
            handle = brick['handle']
            if not component(brick, 'BoxCollider2D')['enabled']:
                disabled.add(handle)
                sprite = component(brick, 'SpriteRenderer')['fields']['sprite']
                animations.setdefault(handle, set()).add(json.dumps(sprite, sort_keys=True))
        current = live(snapshot, 'Ball')
        balls.append(dict(frame=row['frame'], time=snapshot['time']['time'],
                          handles=[b['handle'] for b in current],
                          states=[dict(handle=b['handle'], position=position(b), velocity=velocity(b))
                                  for b in current]))
        speeds += [math.hypot(*velocity(b)) for b in current]
        paddle = live(snapshot, 'Platform')[0]
        widths.append(dict(frame=row['frame'], time=snapshot['time']['time'],
                           width=component(paddle, 'BoxCollider2D')['fields']['size'][0],
                           x=position(paddle)[0]))
        times.append(snapshot['time']['time'])
    contacts, death_zone_balls = {}, set()
    events = [o for o in report['observations'] if o['selector'] == 'events']
    for row in events:
        counts = {}
        for event in row['value']:
            if event['phase'] != 'enter':
                continue
            pair = sorted([names.get(event['a'], '?'), names.get(event['b'], '?')])
            if event['trigger'] and pair == ['Ball', 'DeathZone']:
                death_zone_balls.add(event['a'] if names[event['a']] == 'Ball' else event['b'])
            key = '/'.join(pair) + ('/trigger' if event['trigger'] else '/solid')
            counts[key] = counts.get(key, 0) + 1
        for key, count in counts.items():
            contacts[key] = max(contacts.get(key, 0), count)
    return dict(observed_frames=len(rows), native_time=max(times, default=0),
                first_bricks=len(live(rows[0]['value'], 'Brick')) if rows else None,
                final_bricks=len(live(rows[-1]['value'], 'Brick')) if rows else None,
                disabled_bricks=sorted(disabled), removed_bricks=sorted(removed),
                break_sprite_counts={str(h): len(s) for h, s in animations.items()},
                ball_counts=sorted({len(b['handles']) for b in balls}), ball_id_samples=balls,
                widths=widths, speed_range=[min(speeds, default=0), max(speeds, default=0)],
                wall_cells=sorted({len(c['fields']['cells']) for row in rows
                    for obj in row['value']['objects'] for c in obj['components']
                    if c['kind'] == 'TilemapCollider2D' and c['enabled']}),
                loss=loss_evidence(report), width_lifetime=width_evidence(report),
                death_zone_ball_handles=sorted(death_zone_balls),
                contacts=contacts, diagnostics=report['diagnostics'], teardown=report['teardown'])


def verify_record(path, fixture_path):
    record = json.loads(Path(path).read_text())
    fixture = json.loads(Path(fixture_path).read_text())
    assert record['status'] == 'pass', record['messages']
    report = record['worker']
    assert not report['diagnostics'], report['diagnostics']
    assert report['teardown'] == dict(completed=True, native_worlds=0, managed_roots=0, pending_readbacks=0)
    run_dir = ROOT / record['run_dir']
    from PIL import Image
    import hashlib
    for capture in record['captures']:
        image = run_dir / capture['path']
        assert hashlib.sha256(image.read_bytes()).hexdigest() == capture['sha256']
        with Image.open(image) as pixels:
            assert (pixels.width, pixels.height) == (960, 720), 'expected actual 960x720 capture'
            assert len(pixels.convert('RGB').getcolors(10000000)) > 20
    for cp in fixture['metadata']['journey_checkpoints']:
        observed = next(o['value'] for o in report['observations']
                        if o['frame'] == cp['frame'] and o['phase'] == cp['label']
                        and o['selector'] == 'world.snapshot')
        assert len(live(observed, 'Brick')) == cp['brick_count'], cp['label']
        assert len(live(observed, 'Ball')) == cp['ball_count'], cp['label']
    evidence = summarize(report)
    evidence.update(run_id=record['run_id'], profile=record['profile'],
                    captures=record['captures'], fixture=str(fixture_path),
                    complete_journey=False)
    return evidence
