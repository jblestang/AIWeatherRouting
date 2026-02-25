import re
import csv

def main():
    with open('data/imoca_60.html', 'r', encoding='utf-8') as f:
        html = f.read()

    # Get the polar table (the first table with cellpadding=4)
    table_parts = html.split('<table cellpadding=4 cellspacing=0 border=1>')
    if len(table_parts) < 2:
        print("Couldn't find table")
        return
        
    table_html = table_parts[1].split('</table>')[0]

    rows = re.findall(r'<tr>(.*?)</tr>', table_html, re.DOTALL)
    header_row = rows[0]
    headers = re.findall(r'<th>(.*?)</th>', header_row, re.DOTALL)
    headers = [h.strip().replace(' kt', '') for h in headers]
    headers[0] = 'twa/tws'

    data = [headers]

    for row in rows[1:]:
        th = re.findall(r'<th[^>]*>(.*?)</th>', row, re.DOTALL)
        if not th:
            continue
        twa = th[0].strip().replace('&#176;', '')
        
        tds = re.findall(r'<td[^>]*>(.*?)</td>', row, re.DOTALL)
        tds = [td.strip() for td in tds]
        
        data.append([twa] + tds)

    with open('data/imoca_60.csv', 'w', newline='') as f:
        writer = csv.writer(f)
        writer.writerows(data)
        
    print("Parse successful, wrote data/imoca_60.csv")

if __name__ == "__main__":
    main()
