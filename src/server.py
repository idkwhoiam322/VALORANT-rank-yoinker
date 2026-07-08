import json
import logging
import os
from websocket_server import WebsocketServer

from src.constants import version, PROJECT_ROOT

logging.getLogger('websocket_server.websocket_server').disabled = True

# websocket.enableTrace(True)

class Server:
    def __init__(self, log, Error):
        self.Error = Error
        self.log = log
        self.lastMessages = {}

    def start_server(self):
        try:
            with open(os.path.join(PROJECT_ROOT, "config.json"), "r") as conf:
                port = json.load(conf)["port"]
            self.server = WebsocketServer(host="0.0.0.0", port=port)
            
            def on_message_received(client, server, message):
                import sys
                import os
                import json
                try:
                    data = json.loads(message)
                    if data.get("action") == "restart_application":
                        print("\n[INFO] Force Refresh requested. Restarting...")
                        os.execv(sys.executable, ['python'] + sys.argv)
                except Exception:
                    pass

            self.server.set_fn_message_received(on_message_received)

            self.server.set_fn_new_client(self.handle_new_client)
            self.server.run_forever(threaded=True) # Now the server is ready to listen
        except Exception as e:
            self.Error.PortError(port)
        
        

    def handle_new_client(self, client, server):
        self.send_payload("version",{
            "core": version
        })
        for key in self.lastMessages:
            if key not in ["chat", "version", "state_change"]:
                self.send_message(self.lastMessages[key])

    def send_message(self, message):
        self.server.send_message_to_all(message)

    def send_payload(self, type, payload):
        payload["type"] = type
        msg_str = json.dumps(payload)
        self.lastMessages[type] = msg_str
        self.server.send_message_to_all(msg_str)
