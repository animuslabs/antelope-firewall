import requests
import time

for i in range(100):
    time.sleep(1)
    try:
        response = requests.post('http://127.0.0.1:3000/get_account', json={'name': 'John', 'age': 30})
        print(response.json())
    except Exception:
        print(response)

    try:
        response = requests.post('http://127.0.0.1:3000/push_transaction', json={'name': 'John', 'age': 30})
        print(response.json())
    except Exception:
        print(response)
