from flask import Flask, jsonify, request

app = Flask(__name__)

@app.route('/', defaults={'path': ''}, methods = ['POST', 'GET'])
@app.route('/<path:path>', methods = ['POST', 'GET'])
def catch_all(path):
    if request.method == 'POST':
        if path == "push_transaction":
            data = request.get_json()
            return jsonify({ 'result': f"You made a post req into path /{path}", 'data': data})
        else: 
            return "Bad Request", 400
    elif request.method == 'GET':
        return jsonify({ 'result': f"You made a get req into path /{path}"})
