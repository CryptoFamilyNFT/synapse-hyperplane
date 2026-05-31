#!/usr/bin/env python3
"""Generate fake Agave account files for testing Synapse Hyperplane"""

import os
import struct
import random
import sys

def generate_account_file(output_path, num_accounts=100):
    """Generate a fake account file with specified number of accounts"""
    
    print(f"Generating {num_accounts} accounts to {output_path}")
    
    with open(output_path, 'wb') as f:
        for i in range(num_accounts):
            # Generate random account data
            pubkey = os.urandom(32)
            owner = os.urandom(32)
            lamports = random.randint(1000, 1000000)
            slot = random.randint(100, 10000)
            rent_epoch = random.randint(0, 100)
            executable = random.choice([0, 1])
            
            # Account data (random size 100-1000 bytes)
            data_len = random.randint(100, 1000)
            data = os.urandom(data_len)
            
            # Calculate padding for 8-byte alignment
            total_size = 8 + 32 + 32 + 1 + 8 + 8 + 8 + data_len
            padding = (8 - (total_size % 8)) % 8
            
            # Write account record
            stored_size = 97 + data_len  # metadata + data
            
            # Stored size (u64)
            f.write(struct.pack('<Q', stored_size))
            
            # Pubkey (32 bytes)
            f.write(pubkey)
            
            # Owner (32 bytes)
            f.write(owner)
            
            # Executable (u8)
            f.write(struct.pack('<B', executable))
            
            # Rent epoch (u64)
            f.write(struct.pack('<Q', rent_epoch))
            
            # Lamports (u64)
            f.write(struct.pack('<Q', lamports))
            
            # Slot (u64)
            f.write(struct.pack('<Q', slot))
            
            # Data
            f.write(data)
            
            # Padding
            if padding > 0:
                f.write(b'\x00' * padding)
    
    print(f"Generated {output_path} ({os.path.getsize(output_path)} bytes)")

def main():
    if len(sys.argv) < 2:
        print("Usage: generate_test_accounts.py <output_dir> [num_files] [accounts_per_file]")
        sys.exit(1)
    
    output_dir = sys.argv[1]
    num_files = int(sys.argv[2]) if len(sys.argv) > 2 else 5
    accounts_per_file = int(sys.argv[3]) if len(sys.argv) > 3 else 100
    
    os.makedirs(output_dir, exist_ok=True)
    
    # Generate account files with Agave naming convention (24-digit epoch.slot)
    for i in range(num_files):
        epoch = 0
        slot = i * 1000
        filename = f"{epoch:016d}.{slot:08d}"
        output_path = os.path.join(output_dir, filename)
        generate_account_file(output_path, accounts_per_file)
    
    print(f"\nGenerated {num_files} account files in {output_dir}")
    print(f"Total accounts: {num_files * accounts_per_file}")

if __name__ == '__main__':
    main()
