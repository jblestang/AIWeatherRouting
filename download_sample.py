import urllib.request
from datetime import datetime
import os

# Create data directory if it doesn't exist
os.makedirs("data", exist_ok=True)

# Using NOAA NOMADS GFS as a reliable alternative for a sample GRIB2 file
# It's an open, unauthenticated source for global weather data (Wind, Current, etc.)
print("Downloading sample GFS GRIB2 file from NOAA NOMADS...")

# A small subset of GFS data (e.g., 1 degree resolution, just wind/pressure)
# Downloading a historical file from a reliable archive to avoid rolling window issues
url = "https://noaa-gfs-bdp-pds.s3.amazonaws.com/gfs.20231201/00/atmos/gfs.t00z.pgrb2.1p00.f000"

output_file = "data/sample_gfs.grib2"

try:
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    with urllib.request.urlopen(req) as response, open(output_file, 'wb') as out_file:
        data = response.read()
        out_file.write(data)
    print(f"Successfully downloaded sample to {output_file}")
except Exception as e:
    print(f"Error downloading file: {e}")
