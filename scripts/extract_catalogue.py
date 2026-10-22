"""Extract only template catalogue sheets; never import community report data."""
from pathlib import Path
from openpyxl import load_workbook
import hashlib,json,sys
source=Path(sys.argv[1]);w=load_workbook(source,data_only=True)
vehicles={}
for sheet,kind,start in [('Vehicle Templates','body',5),('Window Templates','window',4)]:
 s=w[sheet];team=''
 for row in list(s.rows)[1:]:
  name=row[0].value
  if not name:
   if row[1].value:team=str(row[1].value).split('     ')[0].strip()
   continue
  raw=str(name);name=raw.lstrip('*').strip();key=(team,name)
  v=vehicles.setdefault(key,dict(id=hashlib.sha256((team+'|'+name).encode()).hexdigest()[:20],name=name,team=team,source='Livery Templates Status.xlsx',liverySupport='unknown',bodyStatus=None,windowStatus=None,notes=[],slots=[],sourceRows=[]))
  v[kind+'Status']=row[1].value
  v['sourceRows'].append(f'{sheet}!A{row[0].row}')
  if row[0].comment:v['notes'].append(row[0].comment.text)
  for i in range(start-1,len(row)):
   if row[i].value is not None and row[i].value!='N/A':
    side=str(s.cell(1,i+1).value);v['slots'].append(dict(key=(('Window ' if kind=='window' else '')+side),status=str(row[i].value),kind=kind))
  if kind=='body' and row[1].value in ['Completed','In Progress']:v['liverySupport']='supported'
data=dict(capturedAt='2026-10-22',updatedAt='2026-10-07',source='User-supplied official Livery Templates Status.xlsx',sourceSha256=hashlib.sha256(source.read_bytes()).hexdigest(),completeness='Official template catalogue snapshot; blank statuses remain unspecified.',license='Source supplied for catalogue integration; distribution terms require confirmation before release.',vehicles=list(vehicles.values()))
Path('catalogue/official-vehicles.json').write_text(json.dumps(data,indent=2,ensure_ascii=False),encoding='utf-8')
print(f'{len(vehicles)} distinct team/name entries; source workbook unchanged.')
