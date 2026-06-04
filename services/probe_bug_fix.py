import requests, json, time, urllib.parse
url='http://127.0.0.1:4000/search'
cases=[
 'excluding pytorch tensorflow mxnet',
 'without luks or zfs',
 'type safety in rust',
 'high performance async runtime in rust',
 'comprehensive production ready event driven microservices architecture using kubernetes docker containers grpc kafka event streaming postgresql for persistence redis for caching prometheus for metrics grafana for dashboards jaeger for distributed tracing with circuit breakers rate limiters and retries',
 'latest CVE vulnerabilities affecting openssl 3 branch after 2024',
 'linux filesystem encryption without luks or zfs',
 'portable database engine for embedded devices without postgres without mysql',
]
for q in cases:
    t0=time.time()
    r=requests.get(url,params={'q':q},timeout=45)
    dt=round(time.time()-t0,3)
    d=r.json()
    sc=d.get('structured_constraints',{})
    print('Q:', q[:120])
    print(' intent=',d.get('intent'),'conf=',round(d.get('confidence') or 0,3),'ms=',dt,'count=',len(d.get('results') or []))
    print(' positive=',sc.get('positive'))
    print(' negative=',sc.get('negative'))
