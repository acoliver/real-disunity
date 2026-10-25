"""Add source-rule and native contact/clock assertions before immutable harness replay."""
import argparse
import copy
import json
from pathlib import Path
from driver import ROOT
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


def align_record(fixture, record_path):
    record = json.loads(record_path.read_text())
    request = json.loads((ROOT / record['run_dir'] / 'request.json').read_text())
    assert request['timeline'] == fixture['timeline']
    assert not record['worker']['diagnostics']
    assert all(a['passed'] for a in record['assertions'] if a['assertion'].get('selector') != 'events')
    aligned = align_contact_assertions(fixture, record['worker']['observations'])
    return aligned


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--checkpoints', type=Path, required=True)
    parser.add_argument('--contact-record', type=Path, help='Align contact pointers to a canonical replay after event filtering changes.')
    args = parser.parse_args()
    path = args.fixture
    fixture = json.loads(path.read_text())
    checkpoints = json.loads(args.checkpoints.read_text())
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
        fixture = align_record(fixture, args.contact_record)
    fixture = copy.deepcopy(fixture)
    path.write_text(json.dumps(fixture, indent=2) + '\n')
    print(json.dumps(dict(fixture=str(path), assertions=len(fixture['assertions']),
                         frames=max(a['frame'] for a in fixture['timeline']))))


if __name__ == '__main__':
    main()
