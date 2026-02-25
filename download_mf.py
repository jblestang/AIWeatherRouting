import urllib.request
import os
import time

os.makedirs("data", exist_ok=True)
url = "https://donneespubliques.meteofrance.fr/donnees_libres/Txt/Arome/arome_0.025_SP1_00H_24H.grib2"
output_file = "data/arome_sample.grib2"

# Use common headers to bypass simple WAF rules
headers = {
    'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36',
    'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8',
    'Accept-Language': 'en-US,en;q=0.9,fr;q=0.8',
}

try:
    print(f"Downloading AROME GRIB2 from Meteo France...")
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as response:
        with open(output_file, 'wb') as out_file:
            # chunked download to show progress
            while chunk := response.read(8192):
                out_file.write(chunk)
                
    print(f"Successfully downloaded to {output_file}")
except Exception as e:
    print(f"Error downloading file: {e}")
