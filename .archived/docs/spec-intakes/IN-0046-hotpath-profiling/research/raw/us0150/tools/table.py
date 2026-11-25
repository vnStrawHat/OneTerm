"""One row per run: steady Mcycles/s, frames/s, Mcycles per frame, key render calls per frame."""
import glob
import json
import os
import re
import sys

for d in sys.argv[1:]:
    for log in sorted(glob.glob(os.path.join(d, '*.log'))):
        label = os.path.basename(log)[:-4]
        js = os.path.join(d, label + '.json')
        text = open(log, encoding='utf-8', errors='replace').read()
        m = re.search(r'([\d.,]+) Mcycles/s; tid (\d+)', text)
        if not m or not os.path.exists(js):
            print(label, 'incomplete')
            continue
        mcs = float(m.group(1).replace(',', ''))
        tid = int(m.group(2))
        rep = json.load(open(js, encoding='utf-8'))
        body = rep['functions_timing']
        secs = body['total_elapsed_ns'] / 1e9
        rows = {r['name']: r for r in body['data']}

        def calls(suffix):
            return next((r['calls'] for n, r in rows.items() if n.endswith(suffix)), 0)

        fr = calls('OneTermWorkspace::render')
        ui = next((t['cpu_percent_avg'] for t in rep['threads']['data'] if t['os_tid'] == tid), '?')
        fps = fr / secs
        print(f"{label:34s} {mcs:7.2f} Mc/s  {fps:6.2f} fr/s  {mcs / fps:6.2f} Mc/frame  hp-ui {ui:>5s}  "
              f"status {calls('build_status_bar') / fr:4.2f}/fr  title {calls('TitleBarContent::render') / fr:4.2f}/fr  "
              f"term {calls('TerminalView::render') / fr:4.2f}/fr")
