"""Qualify the complete multi-seed journey from fresh-source native GPU replays."""
import hashlib
import json
from pathlib import Path
from driver import ROOT, WORK, dump, live, position, replay_fixture
from verify import clamp_evidence, native_identity, verify_record

SCENARIOS = ['multiball-loss', 'controls-complete', 'wide-complete']
VARIANTS = [(p, desktop) for p in ['compat-2017.1', 'compat-6000.3']
            for desktop in [False, True]]


def file_identity(path):
    return dict(path=str(path.relative_to(ROOT)),
                sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def renewed_control(report):
    rows = {o['phase']: o['value'] for o in report['observations']
            if o['selector'] == 'world.snapshot'}
    reloaded = rows['second-reload']
    renewed = rows['renewed-control']
    return (not live(reloaded, 'Ball') and not live(reloaded, 'Brick')
            and len(live(renewed, 'Brick')) == 48 and len(live(renewed, 'Ball')) == 1
            and renewed['input']['keys'] == ['D']
            and abs(position(live(renewed, 'Platform')[0])[0] - .4) < .001)


def compare_captures(results):
    from PIL import Image, ImageChops
    comparisons, gaps = [], []
    for scenario in SCENARIOS:
        scenario_runs = [run for run in results if scenario in Path(run['fixture']).stem]
        image_pairs, identical_pairs = 0, 0
        for index, left in enumerate(scenario_runs):
            left_dir = ROOT / '.work/compat/runs' / left['run_id']
            for right in scenario_runs[index + 1:]:
                right_dir = ROOT / '.work/compat/runs' / right['run_id']
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


def main():
    results, gaps = [], []
    for profile, desktop in VARIANTS:
        label = profile + ('-desktop' if desktop else '-offscreen')
        summaries, records = {}, {}
        for scenario in SCENARIOS:
            fixture_path = ROOT / f'fixtures/breakout-journey-{scenario}-compact.json'
            fixture = json.loads(fixture_path.read_text())
            assert not Path(fixture['source_root']).is_absolute()
            assert fixture['metadata']['script_count'] == 7
            assert 'assembly' not in fixture['metadata']
            assert 'assembly_qualification' not in fixture['metadata']
            assert fixture['limits']['deadline_ms'] == 120000
            # Revalidate the public stimulus contract without changing the frozen file.
            replay_fixture(fixture, fixture['timeline'][:-1],
                           fixture['timeline'][-1]['frame'], scenario, {})
            folder = WORK / 'replays' / fixture_path.stem
            record_path = folder / (label + '.json')
            verified_path = folder / (label + '-verified.json')
            if not record_path.exists() or not verified_path.exists():
                gaps.append(f'{label}/{scenario}: missing successful verified replay')
                continue
            try:
                record = json.loads(record_path.read_text())
                prior = json.loads(verified_path.read_text())
                assert prior['fixture_sha256'] == hashlib.sha256(fixture_path.read_bytes()).hexdigest()
                assert prior['run_id'] == record['run_id']
                summary = verify_record(record_path, fixture_path)
                run_dir = (ROOT / record['run_dir']).resolve()
                summary.update(native_image=native_identity(run_dir), desktop=desktop,
                    fixture_identity=file_identity(fixture_path),
                    managed_images=[file_identity(run_dir / 'build/bridge/bin/CompatibilityBridge/debug' / name)
                                    for name in ['CompatibilityBridge.dll', 'UnityCompat.dll']],
                    elapsed_ms=record['elapsed_ms'])
                summaries[scenario] = summary
                records[scenario] = record
                results.append(summary)
            except (AssertionError, KeyError, OSError, ValueError) as error:
                gaps.append(f'{label}/{scenario}: {error}')
        if len(summaries) != len(SCENARIOS):
            continue
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
            normal_clamps=clamp_evidence(controls['widths'], 2),
            wide_clamps=clamp_evidence(wide['widths'], 4),
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
        dump(WORK / f'qualification-{label}.json', dict(checks=checks,
             run_ids=[s['run_id'] for s in summaries.values()]))
    capture_comparisons, pixel_gaps = compare_captures(results)
    gaps.extend(pixel_gaps)
    report = dict(status='complete' if not gaps else 'incomplete',
        scope='three seeded keyboard scenarios, each in both profiles and presentation modes',
        single_timeline=False, complete_journey_suite=not gaps, missing_requirements=gaps,
        capture_pixel_comparisons=capture_comparisons,
        translation='original-source/unlit-material-translation/sprite-bounds-collision',
        width_extension_semantics='Observed actual brick break and width4 past the initial authored8s duration, '
            'then normal width2. Original Brick.BreakRoutine calls PowerUp.Apply after break animation; '
            'PowerUp.Apply directly invokes Platform.MakeWider for the wide result. No falling collectible, '
            'private timer or direct powerup invocation is used.',
        powerup_source_hashes={name: json.loads((ROOT / 'fixtures/breakout-journey-wide-complete-compact.json').read_text())
            ['metadata']['source_sha256'][f'Assets/Scripts/{name}.cs'] for name in ['Brick', 'PowerUp']},
        runs=results)
    dump(WORK / 'journey-qualification.json', report)
    print(json.dumps(dict(status=report['status'], runs=len(results), missing_requirements=gaps), indent=2))
    if gaps:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
