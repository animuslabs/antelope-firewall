import requests
import time

try:
    response = requests.post('https://mainnet.genereos.io/v1/chain/get_raw_abi', json={
        "account_name": "eosio"
    }, headers={
        "User-Agent": "curl/8.2.1",
        "Accept": "*/*",
        "Host": "127.0.0.1:3000",
    })
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
