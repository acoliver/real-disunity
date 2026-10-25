"""Check the complete multi-seed journey from fresh-source native GPU replays."""
import argparse
import json
from pathlib import Path
from driver import ROOT, live, position, replay_fixture
from verify import clamps_reach_edges, verify_record

SCENARIOS = ['multiball-loss', 'controls-complete', 'wide-complete']
VARIANTS = [(p, desktop) for p in ['compat-2017.1', 'compat-6000.3']
            for desktop in [False, True]]


def renewed_control(report):
    rows = {o['phase']: o['value'] for o in report['observations']
            if o['selector'] == 'world.snapshot'}
    reloaded = rows['second-reload']
    renewed = rows['renewed-control']
    return (not live(reloaded, 'Ball') and not live(reloaded, 'Brick')
            and len(live(renewed, 'Brick')) == 48 and len(live(renewed, 'Ball')) == 1
            and renewed['input']['keys'] == ['D']
            and abs(position(live(renewed, 'Platform')[0])[0] - .4) < .001)


def compare_captures(results, root=ROOT):
    from PIL import Image, ImageChops
    comparisons, gaps = [], []
    for scenario in SCENARIOS:
        scenario_runs = [run for run in results if scenario in Path(run['fixture']).stem]
        image_pairs, identical_pairs = 0, 0
        for index, left in enumerate(scenario_runs):
            left_dir = root / left['run_dir']
            for right in scenario_runs[index + 1:]:
                right_dir = root / right['run_dir']
                right_captures = {capture['path']: capture for capture in right['captures']}
                for capture in left['captures']:
                    matching = right_captures.get(capture['path'])
                    if matching is None:
                        gaps.append(f'{scenario}: unmatched capture {capture["path"]}')
                        continue
                    image_pairs += 1
                    left_path, right_path = left_dir / capture['path'], right_dir / matching['path']
                    with Image.open(left_path) as left_image, Image.open(right_path) as right_image:
                        if left_image.size != (960, 720) or right_image.size != (960, 720):
                            gaps.append(f'{scenario}: non-960x720 pixel pair {capture["path"]}')
                            continue
                        same = not ImageChops.difference(
                            left_image.convert('RGB'), right_image.convert('RGB')).getbbox()
                    if same:
                        identical_pairs += 1
                    else:
                        gaps.append(f'{scenario}: pixel mismatch for {capture["path"]} '
                                    f'between {left["run_id"]} and {right["run_id"]}')
        comparisons.append(dict(scenario=scenario, run_pairs=len(scenario_runs) *
            (len(scenario_runs) - 1) // 2, image_pairs=image_pairs,
            pixel_identical_pairs=identical_pairs))
    return comparisons, gaps


def scenario_requirements(summaries, records, label):
    gaps = []
    loss, controls, wide = [summaries[s] for s in SCENARIOS]
    contacts = {key for s in summaries.values() for key, count in s['contacts'].items() if count}
    checks = dict(
        original_layout=all(s['first_bricks'] == 48 and s['wall_cells'] == [48]
                            for s in summaries.values()),
        original_speed=all(abs(s['speed_range'][0] - 6) < .001
                           and abs(s['speed_range'][1] - 6) < .001
                           for s in summaries.values()),
        initial_upward_launch=all(any(b['velocity'][1] > 0
            for b in s['ball_id_samples'][0]['states']) for s in summaries.values()),
        ten_break_sprites_and_removal=any(count >= 10 and int(handle) in s['removed_bricks']
            for s in summaries.values() for handle, count in s['break_sprite_counts'].items()),
        normal_clamps=clamps_reach_edges(controls['widths'], 2),
        wide_clamps=clamps_reach_edges(wide['widths'], 4),
        actual_wide_expiry=wide['width_lifetime']['expired'],
        observed_width_extension=wide['width_lifetime']['observed_past_initial_duration'],
        no_premature_respawn=loss['loss']['no_premature_respawn'],
        last_loss_fresh_launch=loss['loss']['last_loss_fresh_launch'],
        lost_ball_trigger_identity=(set(loss['loss'].get('removed', []))
            | {loss['loss'].get('survivor')}) <= set(loss['death_zone_ball_handles']),
        physical_brick_contact='Ball/Brick/solid' in contacts,
        physical_wall_contact='Ball/Tilemap/solid' in contacts,
        physical_paddle_contact='Ball/Platform/solid' in contacts,
        physical_death_zone='Ball/DeathZone/trigger' in contacts,
        reload_and_renewed_keyboard=renewed_control(records['controls-complete']['worker']),
        actual_captures=all(len(s['captures']) > 0 for s in summaries.values()))
    for name, passed in checks.items():
        if not passed:
            gaps.append(f'{label}: {name}')
    return gaps


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--replays', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=ROOT / 'fixtures')
    args = parser.parse_args()
    results, gaps = [], []
    for profile, desktop in VARIANTS:
        label = profile + ('-desktop' if desktop else '-offscreen')
        summaries, records = {}, {}
        for scenario in SCENARIOS:
            fixture_path = args.fixtures / f'breakout-journey-{scenario}-compact.json'
            fixture = json.loads(fixture_path.read_text())
            assert fixture['metadata']['script_count'] == 7
            assert fixture['limits']['deadline_ms'] == 120000
            # Revalidate the public stimulus contract without changing the frozen file.
            replay_fixture(fixture, fixture['timeline'][:-1],
                           fixture['timeline'][-1]['frame'], scenario, {})
            folder = args.replays / fixture_path.stem
            record_path = folder / (label + '.json')
            if not record_path.exists():
                gaps.append(f'{label}/{scenario}: missing successful verified replay')
                continue
            try:
                record = json.loads(record_path.read_text())
                summary = verify_record(record_path, fixture_path)
                summary.update(desktop=desktop, elapsed_ms=record['elapsed_ms'], run_dir=record['run_dir'])
                summaries[scenario] = summary
                records[scenario] = record
                results.append(summary)
            except (AssertionError, KeyError, OSError, ValueError) as error:
                gaps.append(f'{label}/{scenario}: {error}')
        if len(summaries) != len(SCENARIOS):
            continue
        gaps.extend(scenario_requirements(summaries, records, label))
    capture_comparisons, pixel_gaps = compare_captures(results)
    gaps.extend(pixel_gaps)
    print(json.dumps(dict(status='incomplete' if gaps else 'complete', runs=len(results), missing_requirements=gaps, capture_pixel_comparisons=capture_comparisons), indent=2))
    if gaps:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
