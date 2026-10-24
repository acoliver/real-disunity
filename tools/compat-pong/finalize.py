"""Add source-rule and native contact/clock assertions before immutable harness replay."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
from driver import ROOT, WORK, fresh_fixture
from verify import contact, verify_wait


def align_contact_assertions(fixture, observations):
    result = copy.deepcopy(fixture)
    checkpoints = fixture['metadata']['journey_checkpoints']
    for cp in checkpoints:
        label, frame = cp['label'], cp['frame']
        if not (label.endswith('-bounce') or label.endswith('-goal-reset')):
            continue
        prefix = '-'.join(label.split('-')[:2]) + '-'
        bounce = next(c['label'] for c in checkpoints if c['label'].startswith(prefix) and c['label'].endswith('-bounce'))
        right = '-right-' in bounce
        trigger = label.endswith('-goal-reset')
        other = ('PlayerOneGoal' if right else 'PlayerTwoGoal') if trigger else ('Enemy' if right else 'Player')
        def observed(selector):
            return next(o['value'] for o in observations if o['frame'] == frame
                        and o['phase'] == 'observe' and o['selector'] == selector)
        events = observed('events')
        event = contact(events, observed('world.snapshot'), other, trigger)
        selected = [a for a in result['assertions'] if a.get('selector') == 'events'
                    and a.get('frame') == frame and a.get('phase') == 'observe']
        assert len(selected) == 2, ('expected exact phase/trigger assertions', label)
        assert {a['pointer'].split('/')[-1] for a in selected} == {'phase', 'trigger'}
        for assertion in selected:
            field = assertion['pointer'].split('/')[-1]
            assert assertion['op'] == 'equal' and assertion['value'] == event[field]
            assertion['pointer'] = f'/{events.index(event)}/{field}'
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--contact-record', type=Path, help='Align contact pointers to a canonical replay after event filtering changes.')
    args = parser.parse_args()
    path = ROOT / 'fixtures/pong-journey.json'
    fixture = json.loads(path.read_text())
    checkpoints = json.loads((WORK / 'planned-checkpoints.json').read_text())
    by_label = {c['label']: c for c in checkpoints}
    def append(assertion):
        if assertion not in fixture['assertions']:
            fixture['assertions'].append(assertion)
    def add(selector, frame, pointer, value):
        append(dict(op='equal', selector=selector, frame=frame,
                    phase='observe', pointer=pointer, value=value))
    for cp in checkpoints:
        append(dict(op='tolerance', selector='world.snapshot', frame=cp['frame'],
            phase='observe', pointer='/time/time', value=cp['snapshot']['time']['time'], tolerance=0.00001))
        if cp['label'].endswith('-bounce') or cp['label'].endswith('-goal-reset'):
            round_prefix = cp['label'].split('-')[0] + '-' + cp['label'].split('-')[1] + '-'
            bounce_label = next(c['label'] for c in checkpoints if c['label'].startswith(round_prefix) and c['label'].endswith('-bounce'))
            side = 'right' if '-right-' in bounce_label else 'left'
            trigger = cp['label'].endswith('-goal-reset')
            other = ('PlayerOneGoal' if side == 'right' else 'PlayerTwoGoal') if trigger else ('Enemy' if side == 'right' else 'Player')
            if any(a.get('selector') == 'events' and a.get('frame') == cp['frame'] for a in fixture['assertions']):
                continue
            event = contact(cp['events'], cp['snapshot'], other, trigger)
            index = cp['events'].index(event)
            add('events', cp['frame'], f'/{index}/phase', 'enter')
            add('events', cp['frame'], f'/{index}/trigger', trigger)
    for i in range(1, 3):
        goal = by_label[f'round-{i}-goal-reset']['snapshot']
        verify_wait(goal, by_label[f'round-{i}-before-deadline']['snapshot'],
                    by_label[f'round-{i}-relaunched']['snapshot'], 2)
    if args.contact_record:
        record = json.loads(args.contact_record.read_text())
        request = json.loads((ROOT / record['run_dir'] / 'request.json').read_text())
        assert request['timeline'] == fixture['timeline']
        assert record['source_before']['scripts'] == record['source_after']['scripts']
        assert not record['worker']['diagnostics']
        assert all(a['passed'] for a in record['assertions'] if a['assertion'].get('selector') != 'events')
        aligned = align_contact_assertions(fixture, record['worker']['observations'])
        changes = [dict(frame=a['frame'], old=a['pointer'], new=b['pointer'], value=a['value'])
                   for a, b in zip(fixture['assertions'], aligned['assertions']) if a != b]
        audit = WORK / 'final-native/contact-alignment.json'
        audit.parent.mkdir(parents=True, exist_ok=True)
        audit.write_text(json.dumps(dict(record=str(args.contact_record), changes=changes), indent=2) + '\n')
        fixture = aligned
    fixture = fresh_fixture(fixture)
    fixture['metadata']['timeline_sha256'] = hashlib.sha256(json.dumps(fixture['timeline'], sort_keys=True,
                                                                          separators=(',', ':')).encode()).hexdigest()
    path.write_text(json.dumps(fixture, indent=2) + '\n')
    print(json.dumps(dict(fixture=str(path), assertions=len(fixture['assertions']),
                         frames=max(a['frame'] for a in fixture['timeline']), metadata=fixture['metadata']['timeline_sha256'])))


if __name__ == '__main__':
    main()
