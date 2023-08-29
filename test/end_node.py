from flask import Flask, jsonify, request
from datetime import datetime, timedelta, timezone

app = Flask(__name__)

@app.route('/', defaults={'path': ''}, methods = ['POST', 'GET'])
@app.route('/<path:path>', methods = ['POST', 'GET'])
def catch_all(path):
    if request.method == 'POST':
        if path == "push_transaction":
            data = request.get_json()
            return jsonify({ 'result': f"You made a post req into path /{path}", 'data': data})
        elif path == "get_info":
            return jsonify({
                "server_version":"3c9661e6",
                "chain_id":"aca376f206b8fc25a6ed44dbdc66547c36c6c33e3a119ffbeaef943642f0e906",
                "head_block_num":313643648,
                "last_irreversible_block_num":313643316,
                "last_irreversible_block_id":"12b1d1343c8fd2cf19985696df6c5ee19b65de572b6a44b7c18eb456451f68c0",
                "head_block_id":"12b1d2802bbebaba1614e892839c31dfca9642ad14d75af0fa1e64f2e5d194de",
                "head_block_time":(datetime.now(timezone.utc) - timedelta(seconds=1)).isoformat(),
                "head_block_producer":"eosasia11111",
                "virtual_block_cpu_limit":200000,
                "virtual_block_net_limit":1048576000,
                "block_cpu_limit":200000,
                "block_net_limit":1048576,
                "server_version_string":"v3.1.0",
                "fork_db_head_block_num":313643648,
                "fork_db_head_block_id":"12b1d2802bbebaba1614e892839c31dfca9642ad14d75af0fa1e64f2e5d194de",
                "server_full_version_string":"v3.1.0-3c9661e67e5f66234871f967f28d1662bf1905b6",
                "total_cpu_weight":"383553445342197",
                "total_net_weight":"96262689186997",
                "earliest_available_block_num":313469889,
                "last_irreversible_block_time":"2023-06-05T23:05:11.500"
            }), 200
        else: 
            return "Bad Request", 400
    elif request.method == 'GET':
        return jsonify({ 'result': f"You made a get req into path /{path}"})
