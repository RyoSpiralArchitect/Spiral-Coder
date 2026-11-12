"""Frozen prompt-only Observer screen. Six calls, no retry, no tool execution."""
import json,hashlib,time,os,sys,urllib.request,urllib.error
from pathlib import Path
A=Path(sys.argv[1]).resolve();plan=json.loads((A/'plan.json').read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(Path(__file__))==plan['runner_sha256']
assert not (A/'started.json').exists(), 'A started probe cannot be rerun'
key=os.environ['OPENAI_API_KEY'];start=time.monotonic()
(A/'started.json').write_text(json.dumps({'plan_sha256':sha(A/'plan.json'),'started_unix':time.time()})+'\n')
for case,condition in plan['order']:
 name=f'{case}.{condition}'; source=A/f'{name}.json'
 assert sha(source)==plan['prepared_sha256'][name]
 if time.monotonic()-start>=plan['wall_seconds']:break
 prepared=json.loads(source.read_text());prompt=prepared['prompt']
 body={'model':plan['model'],'messages':[{'role':'system','content':plan['system']},{'role':'user','content':prompt}],'max_completion_tokens':plan['max_completion_tokens'],'response_format':{'type':'json_object'}}
 # Same JSON-object transport for all six calls, including diagnostic prompts.
 # This screen does not exercise the production strict diagnostic schema.
 t=time.monotonic();record={'case':case,'condition':condition,'request_sha256':hashlib.sha256(json.dumps(body).encode()).hexdigest(),'prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest()}
 try:
  request=urllib.request.Request('https://api.openai.com/v1/chat/completions',data=json.dumps(body).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+key})
  with urllib.request.urlopen(request,timeout=min(90,plan['wall_seconds']-(t-start))) as response: raw=json.loads(response.read())
  record.update(response=raw,success=True)
 except urllib.error.HTTPError as error:
  record.update(success=False,http_status=error.code,error=error.read().decode()[:2000])
 except Exception as error:record.update(success=False,error=type(error).__name__+': '+str(error))
 record['wall_seconds']=round(time.monotonic()-t,3)
 (A/f'{name}.response.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n')
 print(json.dumps({'case':case,'condition':condition,'success':record['success'],'wall_seconds':record['wall_seconds']}),flush=True)
(A/'finished.json').write_text(json.dumps({'wall_seconds':round(time.monotonic()-start,3),'responses':len(list(A.glob('*.response.json'))),'tools_executed':0})+'\n')
