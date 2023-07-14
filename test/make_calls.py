import requests
import time

try:
    response = requests.post('http://127.0.0.1:3000/v1/chain/send_transaction', json={
  "signatures": [
    "SIG_K1_KmRbWahefwxs6uyCGNR6wNRjw7cntEeFQhNCbyg8S92Kbp7zdSSVGTD2QS7pNVWgcU126zpxaBp9CwUxFpRwSnfkjd46bS"
  ],
  "compression": "none",
  "packed_context_free_data": "",
  "packed_trx": "8468635b7f379feeb95500000000010000000000ea305500409e9a2264b89a010000000000ea305500000000a8ed3232660000000000ea305500a6823403ea30550100000001000240cc0bf90a5656c8bb81f0eb86f49f89613c5cd988c018715d4646c6bd0ad3d8010000000100000001000240cc0bf90a5656c8bb81f0eb86f49f89613c5cd988c018715d4646c6bd0ad3d80100000000"
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
