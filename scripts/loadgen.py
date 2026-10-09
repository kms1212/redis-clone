#!/usr/bin/env python3
"""Pipelines one command N times over a single connection and waits for every reply.

    python3 scripts/loadgen.py <port> <count> <command> [args...]

The client only pushes pre-built bytes and discards replies, so it stays cheap enough
that the server's CPU time (see cpubench.sh) reflects the server's own work.
"""
import socket
import sys
import threading

port, count = int(sys.argv[1]), int(sys.argv[2])
args = [arg.encode() for arg in sys.argv[3:]]
frame = b"*%d\r\n" % len(args) + b"".join(b"$%d\r\n%s\r\n" % (len(a), a) for a in args)

sock = socket.create_connection(("127.0.0.1", port))

# Send once to learn the reply size; every repetition gets the same reply.
sock.sendall(frame)
sock.settimeout(1)
reply = b""
while not reply.endswith(b"\r\n"):
    reply += sock.recv(4096)
sock.settimeout(None)

BATCH = 1000
chunk = frame * BATCH
expected = (count // BATCH) * BATCH * len(reply)


def send():
    for _ in range(count // BATCH):
        sock.sendall(chunk)


sender = threading.Thread(target=send)
sender.start()
received = 0
while received < expected:
    data = sock.recv(1 << 20)
    if not data:
        sys.exit("server closed the connection early")
    received += len(data)
sender.join()
sock.close()
