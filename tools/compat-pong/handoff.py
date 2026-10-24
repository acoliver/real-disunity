"""Aggregate independently checked Pong replays and prior-image dynamic contacts."""
import argparse
import hashlib
import json
from pathlib import Path
from driver import ROOT, WORK
from verify import verify_record, verify_runtime


def dynamic_streams(record):
    result = {}
    observations = record['worker']['observations']
    for observation in observations:
        if observation['selector'] != 'events':
            continue
        snapshot = next(o['value'] for o in observations if o['frame'] == observation['frame']
                        and o['phase'] == observation['phase'] and o['selector'] == 'world.snapshot')
        names = {o['handle']: o['name'] for o in snapshot['objects']}
        dynamic = {o['handle'] for o in snapshot['objects'] if o['active_in_hierarchy']
                   and any(c['kind'] == 'Rigidbody2D' and c['enabled']
                           and c['fields']['body_type'] == 'dynamic' and c['fields']['simulated']
                           for c in o['components'])}
        events = [{**e, 'a': names[e['a']], 'b': names[e['b']]} for e in observation['value']
                  if e['a'] in dynamic or e['b'] in dynamic]
        result[observation['frame']] = dict(total=len(observation['value']), dynamic=events)
    return result


def compare_dynamic(before, after):
    old, new = dynamic_streams(before), dynamic_streams(after)
    assert old.keys() == new.keys()
    comparisons = []
    for frame in old:
        assert old[frame]['dynamic'] == new[frame]['dynamic'], ('dynamic contact changed', frame)
        assert new[frame]['total'] == len(new[frame]['dynamic']), ('static-only contact remains', frame)
        digest = hashlib.sha256(json.dumps(new[frame]['dynamic'], sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        comparisons.append(dict(frame=frame, prior_events=old[frame]['total'], corrected_events=new[frame]['total'],
                                dynamic_events_identical=True, dynamic_events_sha256=digest))
    return comparisons


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, default=WORK / 'final-native')
    args = parser.parse_args()
    output = args.output.resolve()
    assert output.is_relative_to(ROOT / '.work')
    matrix = json.loads((output / 'replays.json').read_text())
    labels = {f'{profile}-{mode}' for profile in ['compat-2017.1', 'compat-6000.3']
              for mode in ['headless', 'desktop']} | {'source-2017.1-headless'}
    assert {r['label'] for r in matrix['runs']} == labels and len(matrix['runs']) == 5
    fixture_path = ROOT / 'fixtures/pong-journey.json'
    assert hashlib.sha256(fixture_path.read_bytes()).hexdigest() == matrix['fixture_sha256']
    fixture = json.loads(fixture_path.read_text())
    prior_fixture = json.loads((WORK / 'prior-image/pong-journey.json').read_text())
    assert fixture['timeline'] == prior_fixture['timeline']
    assertion_changes = []
    for old, new in zip(prior_fixture['assertions'], fixture['assertions']):
        if old != new:
            assert old['selector'] == 'events' and {**old, 'pointer': new['pointer']} == new
            assertion_changes.append(dict(frame=old['frame'], old=old['pointer'], new=new['pointer'], value=new['value']))
    assert len(fixture['assertions']) == len(prior_fixture['assertions']) == 366
    assert len(assertion_changes) == 12
    runs, comparisons = [], []
    for entry in matrix['runs']:
        record = json.loads((ROOT / entry['record']).read_text())
        verified = verify_record(ROOT / entry['record'], fixture_path)
        saved = json.loads((ROOT / entry['verified']).read_text())
        verify_runtime(saved['runtime'], matrix['images'])
        for worker in saved['runtime']['workers']:
            assert worker['native_map_lines'] and worker['native_sha256'] == matrix['images']['native']['sha256']
            assert worker['native_mapping'] == verified['native_image_proof']['image']
            assert worker['native_sha256'] == verified['native_image_proof']['sha256']
            assert worker['player_sha256'] == matrix['images']['player']['sha256']
        prior = json.loads((WORK / 'prior-image' / (entry['label'] + '.json')).read_text())
        assert prior['source_before']['scripts'] == record['source_before']['scripts']
        comparisons.append(dict(label=entry['label'], frames=compare_dynamic(prior, record)))
        runs.append(dict(**entry, source_proof=verified['source_proof'],
                         assembly_sha256=verified['assembly']['assembly_sha256'],
                         pe_metadata=verified['assembly']['metadata'],
                         native_image_proof=verified['native_image_proof'], managed_images=verified['managed_images'],
                         gpu_score_regions=verified['gpu_score_regions'], runtime=saved['runtime']))
    assert len({r['source_proof'] for r in runs}) == 5
    assert len({r['native_image_proof']['image'] for r in runs}) == 5
    gpu = json.loads((output / 'gpu-comparison.json').read_text())
    assert len(gpu) == 10 and all(c['pixel_identical'] for c in gpu)
    (output / 'dynamic-pair-comparison.json').write_text(json.dumps(comparisons, indent=2) + '\n')
    cache = json.loads((output / 'cache-qualification.json').read_text())
    assert cache['status'] == 'pass' and cache['count_only_proof_rejected'] and cache['final_fixture_unchanged']
    verify_runtime(cache['runtime'], cache['runtime']['before'])
    assert cache['runtime']['before']['player'] == matrix['images']['player']
    assert cache['runtime']['before']['harness'] == matrix['images']['harness']
    handoff = dict(cache_qualification=cache, fixture=str(fixture_path.relative_to(ROOT)), fixture_sha256=matrix['fixture_sha256'],
                   timeline_sha256=fixture['metadata']['timeline_sha256'], actions=len(fixture['timeline']),
                   max_frame=max(a['frame'] for a in fixture['timeline']), assertions=366,
                   fresh_compilation=True, images=matrix['images'], runs=runs, gpu_comparisons=gpu,
                   contact_pointer_changes=assertion_changes, original_gameplay_scripts=20,
                   original_total_scripts=31, dynamic_pair_comparison=str((output / 'dynamic-pair-comparison.json').relative_to(ROOT)),
                   evidence_label=fixture['evidence_label'], prior_evidence='.work/pong-journey/prior-image',
                   visual_inspection=json.loads((output / 'visual-inspection.json').read_text()),
                   preserved_errors=[str(p.relative_to(ROOT)) for p in sorted(output.rglob('*-error.txt'))])
    (output / 'handoff.json').write_text(json.dumps(handoff, indent=2) + '\n')
    (WORK / 'handoff.json').write_text(json.dumps(handoff, indent=2) + '\n')
    print(json.dumps(dict(status='pass', runs=len(runs), assertions_per_run=366,
                         captures=25, dynamic_pairs='identical', handoff=str(output / 'handoff.json'))))


if __name__ == '__main__':
    main()
