import requests

for i in range(100):
    response = requests.post('http://127.0.0.1:3000/get_account', json={'name': 'John', 'age': 30})
    print(response.json())
    response = requests.post('http://127.0.0.1:3000/push_transaction', json={'name': 'John', 'age': 30})
    print(response.json())
