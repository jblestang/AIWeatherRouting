import struct

def find_grib_messages(filepath):
    """
    Very basic GRIB indicator parser to scan a file for exactly the "GRIB" magic signature,
    skipping broken bytes, and roughly counting messages.
    """
    count = 0
    with open(filepath, 'rb') as f:
        data = f.read()
        
    idx = 0
    while idx < len(data) - 16:
        if data[idx:idx+4] == b'GRIB':
            # Found a GRIB indicator
            count += 1
            
            # Read length
            version = data[idx+7]
            if version == 1:
                length = struct.unpack('>I', b'\x00' + data[idx+4:idx+7])[0]
            elif version == 2:
                length = struct.unpack('>Q', data[idx+8:idx+16])[0]
            else:
                length = 100 # guess and scan
                
            idx += length
        else:
            idx += 1
            
    print(f"Found {count} complete/partial GRIB messages in the truncated file.")
    
if __name__ == "__main__":
    find_grib_messages("data/arpege_sample.grib2")
