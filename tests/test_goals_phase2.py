"""Phase 2 live tests: JWT claim, owner isolation, consents, leaderboard.

Requires the gateway running with IF_GOAL_STORE=supabase and Supabase keys in
../../.env. Throwaway users are created and deleted per run; no committed tokens.

    IF_API_KEY=intentforge-dev-key pytest tests/test_goals_phase2.py -q
"""
import os
import uuid

import pytest
import requests

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
env = {}
for line in open(os.path.join(ROOT, '.env'), encoding='utf-8'):
    line = line.strip()
    if line and not line.startswith('#') and '=' in line:
        k, v = line.split('=', 1)
        env[k] = v

BASE = os.environ.get('INTENTFORGE_BASE', 'http://localhost:4000').rstrip('/')
SB = env['SUPABASE_URL'].rstrip('/')
SECRET = env['SUPABASE_SECRET_KEY'].strip()
PUBLISHABLE = env['SUPABASE_PUBLISHABLE_KEY'].strip()
DEV_KEY = os.environ.get('IF_API_KEY', 'intentforge-dev-key')


def _headers(key):
    return {'apikey': key, 'Content-Type': 'application/json', 'Accept': 'application/json'}


def make_user():
    suffix = uuid.uuid4().hex[:10]
    email = f'phase2-{suffix}@example.com'
    password = 'P2-' + uuid.uuid4().hex + 'aA1!'
    r = requests.post(SB + '/auth/v1/admin/users', headers=_headers(SECRET),
                      json={'email': email, 'password': password, 'email_confirm': True}, timeout=30)
    assert r.status_code in (200, 201), (r.status_code, r.text[:300])
    uid = r.json()['id']
    r = requests.post(SB + '/auth/v1/token?grant_type=password', headers=_headers(PUBLISHABLE),
                      json={'email': email, 'password': password}, timeout=30)
    assert r.status_code == 200, (r.status_code, r.text[:300])
    return uid, r.json()['access_token']


def delete_user(uid):
    requests.delete(SB + '/auth/v1/admin/users/' + uid, headers=_headers(SECRET), timeout=30)


def anon_create(ref, goal):
    r = requests.post(BASE + '/goals', headers={'X-Anonymous-Owner': str(ref)},
                      json={'goal': goal}, timeout=30)
    assert r.status_code == 200, (r.status_code, r.text[:300])
    return r.json()['goal_id']


@pytest.fixture
def user():
    uid, token = make_user()
    yield uid, token
    delete_user(uid)


def test_claim_isolation_and_consents(user):
    uid, token = user
    auth = {'Authorization': 'Bearer ' + token}
    ref = uuid.uuid4()
    other_ref = uuid.uuid4()
    gid = anon_create(ref, 'learn phase two claim ' + uuid.uuid4().hex[:6])
    # leaderboard view requires a generated roadmap
    r = requests.get(BASE + '/goals/' + gid, headers={'X-Anonymous-Owner': str(ref)}, timeout=20)
    questions = requests.post(BASE + '/goals', headers={'X-Anonymous-Owner': str(ref)},
                              json={'goal': 'learn phase two claim ' + gid}, timeout=30).json().get('questions', [])
    answers = [{'question_id': q['id'], 'answer': q['options'][0] if q.get('options') else 'x'}
               for q in questions[:2]]
    if answers:
        requests.post(BASE + f'/goals/{gid}/answers', headers={'X-Anonymous-Owner': str(ref)},
                      json={'answers': answers}, timeout=30)

    # anonymous owner sees its goal, with TTL
    r = requests.get(BASE + '/goals/' + gid, headers={'X-Anonymous-Owner': str(ref)}, timeout=20)
    assert r.status_code == 200 and 'expires_at' in r.json()

    # claim with JWT + ref
    r = requests.post(BASE + '/goals/claim', headers={**auth, 'X-Anonymous-Owner': str(ref)}, timeout=20)
    assert r.status_code == 200, (r.status_code, r.text[:300])
    assert any(g['goal_id'] == gid for g in r.json())

    # claimed user sees it, no expires_at; old capability does not
    r = requests.get(BASE + '/goals/' + gid, headers=auth, timeout=20)
    assert r.status_code == 200 and 'expires_at' not in r.json(), r.text[:300]
    r = requests.get(BASE + '/goals/' + gid, headers={'X-Anonymous-Owner': str(ref)}, timeout=20)
    assert r.status_code == 404, r.text[:300]

    # second user cannot read or mutate it
    uid2, token2 = make_user()
    try:
        auth2 = {'Authorization': 'Bearer ' + token2}
        assert requests.get(BASE + '/goals/' + gid, headers=auth2, timeout=20).status_code == 404
        assert requests.post(BASE + f'/goals/{gid}/phases/1/complete', headers=auth2,
                             json={}, timeout=20).status_code == 404
    finally:
        delete_user(uid2)

    # idempotent
    r = requests.post(BASE + '/goals/claim', headers={**auth, 'X-Anonymous-Owner': str(ref)}, timeout=20)
    assert r.status_code == 200 and r.json() == [], r.text[:300]

    # consent defaults off, then leaderboard opt-in/out
    r = requests.get(SB + '/rest/v1/consents?owner_id=eq.' + uid + '&select=purpose,enabled',
                     headers=_headers(PUBLISHABLE) | {'Authorization': 'Bearer ' + token}, timeout=20)
    assert r.status_code == 200
    assert all(not row['enabled'] for row in r.json())

    r = requests.put(BASE + '/consents/leaderboard', headers=auth, json={'enabled': True}, timeout=20)
    assert r.status_code == 200, (r.status_code, r.text[:300])
    r = requests.get(BASE + '/goals/leaderboard', timeout=20)
    assert r.status_code == 200 and isinstance(r.json(), list)
    entries = [e for e in r.json() if e['goal_id'] == gid]
    assert len(entries) == 1, entries
    assert {'goal_id', 'goal', 'user_name', 'score', 'completed_phases', 'total_phases', 'created_at'} <= set(entries[0])

    r = requests.put(BASE + '/consents/leaderboard', headers=auth, json={'enabled': False}, timeout=20)
    assert r.status_code == 200
    r = requests.get(BASE + '/goals/leaderboard', timeout=20)
    assert all(e['goal_id'] != gid for e in r.json())

    # hiring consent is separate and defaults off
    r = requests.get(SB + '/rest/v1/consents?owner_id=eq.' + uid + '&purpose=eq.hiring_notices&select=enabled',
                     headers=_headers(PUBLISHABLE) | {'Authorization': 'Bearer ' + token}, timeout=20)
    assert r.status_code == 200
    if r.json():
        assert r.json()[0]['enabled'] is False


def test_invalid_credentials():
    ref = uuid.uuid4()
    assert requests.post(BASE + '/goals/claim', headers={'X-Anonymous-Owner': str(ref)}, timeout=20).status_code == 401
    assert requests.get(BASE + '/goals', headers={'Authorization': 'Bearer not-a-jwt'}, timeout=20).status_code == 401
    assert requests.put(BASE + '/consents/bogus', headers={'Authorization': 'Bearer not-a-jwt'},
                        json={'enabled': True}, timeout=20).status_code == 401
