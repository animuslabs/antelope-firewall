from flask import Flask, jsonify, request

app = Flask(__name__)

@app.route('/', defaults={'path': ''}, methods = ['POST', 'GET'])
@app.route('/<path:path>', methods = ['POST', 'GET'])
def catch_all(path):
    if request.method == 'POST':
        data = request.get_json()
        return jsonify({ 'result': f"You made a post req into path /{path}", 'data': data})
    elif request.method == 'GET':
        return jsonify({ 'result': f"You made a get req into path /{path}"})
