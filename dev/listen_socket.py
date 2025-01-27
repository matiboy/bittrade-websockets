import socket

# Define the path to your Unix socket file
socket_path = '/tmp/binance_manager.sock'

# Create a Unix socket
client_socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)

try:
    # Connect to the Unix socket
    client_socket.connect(socket_path)

    # Receive data from the socket (adjust the buffer size if needed)
    data = client_socket.recv(1024)
    print(f"Received: {data.decode('utf-8')}")

finally:
    # Close the socket
    client_socket.close()