import requests
import time

try:
    response = requests.post('http://127.0.0.1:3000/v1/chain/get_account', json={'account_name': "coinbasebase"})
    print("Returned:", response.json())
except Exception:
    print("Excepted:", response)

#for i in range(100):
    ##time.sleep(1)
    ##try:
    ##    response = requests.post('http://127.0.0.1:3000/get_account', json={'name': 'John', 'age': 30})
    ##    print(response.json())
    ##except Exception:
    ##    print(response)

    #try:
        #response = requests.post('http://127.0.0.1:3000/failure', json={'name': 'John', 'age': 30})
        #print(response.json())
    #except Exception:
        #print(response)
