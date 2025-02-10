import asyncio
import os
import socket
import sys

async def connect_to_unix_socket(socket_path: str):
    reader, writer = await asyncio.open_unix_connection(socket_path)
    print("Connected to the Unix socket.", socket_path)
    start_time = asyncio.get_event_loop().time()
    while True:
        data = await reader.readline()
        if not data:
            print("No more data received?")
            break
        print(f"{asyncio.get_event_loop().time() - start_time} {data.decode()}")
        

async def main():
    if len(sys.argv) < 2:
        print("Please provide the Unix socket path as the first argument.")
        sys.exit(1)

    socket_path = sys.argv[1]
    await connect_to_unix_socket(socket_path)

if __name__ == "__main__":
    asyncio.run(main())
