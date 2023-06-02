#!/usr/bin/python

from end_node import app
import threading

def run_app(p):
    app.run(port=5000+p)

for i in range(4):
    t = threading.Thread(target=run_app, args=(i,))
    t.start()

