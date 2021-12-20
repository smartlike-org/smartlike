let pars = null
let slider
let amount
let isChrome
let key
let MutationObserver = window.MutationObserver || window.WebKitMutationObserver;
let promptPasswordManager = true;
let profile = null;


(function () {

    window.addEventListener('load', function () {
        pars = new URLSearchParams(window.location.search)
        if (pars.has("callback_parameters")) {
            parent.postMessage(JSON.parse(pars.get("callback_parameters")), pars.get("callback"))
            return
        }

        document.getElementById("close").addEventListener('click', function () {
            parent.postMessage({
                type: "cancel",
                data: {
                    type: pars.get("type"),
                }
            }, pars.get("callback"))
        }, false)

        let error = checkParameters()
        if (error != "") {
            document.getElementById("content").style.display = 'none'
            let e = document.getElementById("error_container")
            e.innerHTML = error
            e.style.display = "block";
            parent.postMessage({
                type: "resize",
                data: document.body.scrollHeight + 20
            }, "*")
            return
        }
        if (pars.get("type") == "login") {
            document.getElementById("amount_selector").style.display = "none"
            document.getElementById("add_funds").style.display = "none"
            document.getElementById("charts").style.display = "none"
        } else {
            if (pars.has("currency") == false)
                pars.set("currency", "USD")
            if (pars.has("suggest_amount") == false)
                pars.set("suggest_amount", 0.01)
            if (pars.get("type") == "donate") {
                document.getElementById("comment").style.display = "block"
            } else if (pars.get("type") == "subscribe") {
                if (pars.get("fixed") && pars.get("fixed") == 1) {
                    pars.set("title", "Subscribe to " + pars.get("recipient") + " with " + pars.get("suggest_amount") + " " + pars.get("currency") + " a month")
                    document.getElementById("amount_selector").style.display = "none"
                }
                else
                    pars.set("title", "Create monthly recurring donation to " + pars.get("recipient"))
            }

            if (pars.has("add_funds")) {
                document.getElementById("add_funds").addEventListener('click', function () {
                    window.open("https://smartlike.org/donate?default=" + pars.get("add_funds"),'_blank')
                }, false)
            }
            else
                document.getElementById("add_funds").style.display = "none"

            if (pars.has("charts")) {
                document.getElementById("charts").addEventListener('click', function () {
                    window.open("https://smartlike.org/channel/" + pars.get("charts"),'_blank')
                }, false)
            }
            else
                document.getElementById("charts").style.display = "none"

        }
        document.getElementById("password-init-submit").innerHTML = pars.get("type").charAt(0).toUpperCase() + pars.get("type").slice(1)

        amount = document.getElementById("amount")
        slider = document.getElementById("slider")
        key = document.getElementById("password-init")
        console.log(window.location.search)
        var e = document.getElementById("title")
        if (pars.has("title")) {
            e.innerHTML = pars.get("title")
        }

        if (window.chrome) {
            let extId = 'hbeaghhbggdilbedobkfhneaajmnfipc'
            try {
                console.log("querying extension")
                chrome.runtime.sendMessage(
                    extId, {
                        type: "get-account",
                        data: ""
                    },
                    response => {
                        if (response) {
                            profile = response
                            document.getElementById("profile").style.display = 'block'
                            document.getElementById("profile_id").innerHTML = profile.title.length ? profile.title : profile.id
                        }
                        void chrome.runtime.lastError;
                    }
                )
            } catch (e) {}
        }

        if (pars.has("suggest_amount")) {
            amount.value = slider.value = pars.get("suggest_amount")
        } else {
            amount.value = 0.01
            slider.value = 10
        }
        e = document.getElementById("currency")
        if (pars.has("currency")) {
            e.innerHTML = pars.get("currency")
        }

        if (amount && slider) {
            amount.addEventListener('input', function () {
                if (amount.value < 0)
                    amount.value = 0
                slider.value = amount.value;
            }, false)

            slider.addEventListener('input', function () {
                if (slider.value < 0)
                    slider.value = 0
                amount.value = slider.value;
            }, false)
        }

        e = document.getElementById("password-init")
        e.addEventListener('animationstart', function (event) {
            console.log("onAnimationStart " + event.animationName)
            if ("onautofillstart" === event.animationName) {
                submit()
            }
        }, true)

        isChrome = typeof navigator != "undefined" ? /Chrome/.test(navigator.userAgent) && /Google Inc/.test(navigator.vendor) : false
        parent.postMessage({
            type: "resize",
            data: document.body.scrollHeight + 20
        }, pars.get("callback"))
        if (MutationObserver) {
            createMutationObserver();
        }
        document.getElementById("content").style.visibility = "visible"
    })
}())

function checkParameters() {
    if (pars.has("type")) {
        const required = {
            "login": ["title", "token", "callback"],
            "subscribe": ["title", "token", "recipient", "callback"],
            "smartlike": ["title", "recipient", "callback"],
            "donate": ["title", "recipient", "callback"]
        }
        let t = pars.get("type")
        if (t in required) {
            for (let i = 0; i < required[t].length; i++) {
                if (pars.has(required[t][i]) == false)
                    return "Error: missing parameter '" + required[t][i] + "'"
            }
        }
        return ""
    } else
        return "Error: missing parameter 'type'"
}

function submit() {
    document.getElementById("no-funds").style.display = "none"
    let password = ""
    if (key.value.length) {
        password = key.value
        profile = null
    } else if (profile) {
        password = profile.secret
        promptPasswordManager = false
    }

    if (bip39.validateMnemonic(password) == false) {
        document.getElementById("invalid-secret").style.display = "block"
    } else {
        document.getElementById("invalid-secret").style.display = "none"
        checkoutImpl(password)
    }
}

function checkoutImpl(password) {

    if (pars.get("type") == "login") {
        let [publicKey, signature] = signHex(pars.get("token"), password)
        callbackImpl({
            type: "checkout",
            data: {
                state: "ok",
                type: pars.get("type"),
                publicKey: publicKey,
                token: pars.get("token"),
                signature: signature
            }
        })
    } else if (pars.get("type") == "subscribe") {
        const kind = "add_recurring_donation"
        let tx = {
            kind: kind,
            ts: Math.floor(Date.now() / 1000),
            data: JSON.stringify({
                recipient: pars.get("recipient"),
                amount: parseFloat(amount.value),
                threshold: 0,
                currency: pars.get("currency"),
                title: "",
                avatar: "",
                comment: ""
            })
        }
        const tx_str = JSON.stringify(tx)
        const [publicKey, sig] = signHex(tx_str, password)
        const message = {
            jsonrpc: "2.0",
            method: kind,
            id: 1234,
            params: {
                signed_message: {
                    sender: publicKey,
                    signature: sig,
                    data: tx_str,
                },
            },
        }
    
        sendTx(message).then(error => {
            hash(tx_str).then(h => {
                callbackImpl({
                    type: "checkout",
                    data: {
                        state: error.length ? "error" : "ok",
                        type: pars.get("type"),
                        error: error,
                        publicKey: publicKey,
                        amount: parseFloat(amount.value),
                        currency: pars.get("currency"),
                        tx: tx.ts + "." + h
                    }
                })
            })
        })
    } else {
        let target = pars.get("recipient").replace('//www.', '//').replace('//m.', '//').replace('//mobile.', '//')
        console.log("target " + target)
        let data
        if (pars.get("type") == "donate") {
            data = JSON.stringify({
                kind: 6,
                target: target,
                amount: parseFloat(amount.value),
                currency: pars.get("currency"),
                payload: {
                    action: "publish",
                    text: "test donation"
                }
            })
        } else if (pars.get("type") == "smartlike") {
            data = JSON.stringify({
                kind: 0,
                target: target,
                amount: parseFloat(amount.value),
                currency: pars.get("currency"),
            })
        }

        const kind = "like"
        let tx = {
            kind: kind,
            ts: Math.floor(Date.now() / 1000),
            data: data
        }
        const tx_str = JSON.stringify(tx)
        const [publicKey, sig] = signHex(tx_str, password)
        const message = {
            jsonrpc: "2.0",
            method: kind,
            id: 1234,
            params: {
                signed_message: {
                    sender: publicKey,
                    signature: sig,
                    data: tx_str,
                },
            },
        }

        console.log(message)
    
        sendTx(message).then(res => {
            console.log(res);
            if (res.status == "error" && res.data == "unknown key") {
                document.getElementById("add_funds").style.fontWeight = 600
                document.getElementById("no-funds").style.display = "block"
                return
            }
            hash(tx_str).then(h => {
                callbackImpl({
                    type: "checkout",
                    data: {
                        state: res.status,
                        type: pars.get("type"),
                        error: res.data,
                        publicKey: publicKey,
                        amount: parseFloat(amount.value),
                        currency: pars.get("currency"),
                        tx: tx.ts + "." + h
                    }
                })
            })
        })
    }
}

function callbackImpl(parameters) {
    if (parameters) {
        if (promptPasswordManager)
            window.location.href = 'https://embed.smartlike.org/modal.html?callback_parameters=' + encodeURIComponent(JSON.stringify(parameters)) + '&callback=' + pars.get("callback")
        else
            parent.postMessage(parameters, pars.get("callback"))
    }
}

function onAnimationStart(event) {
    console.log("onAnimationStart " + event.animationName)
    if ("onautofillstart" === event.animationName) {
        promptPasswordManager = false
        submit()
    }
}

function onFormSubmit(e) {
    e.preventDefault()
    return false
}

function generateKey() {
    document.getElementById("invalid-secret").style.display = "none"
    document.getElementById("no-funds").style.display = "none"

    let mnemonic = bip39.generateMnemonic()
    document.getElementById("secret").innerHTML = mnemonic

    if (isChrome == false) {
        document.getElementById("password-init").value = mnemonic
        document.getElementById("backup").innerHTML = "Please keep your account key in the password manager and make a back up: "
    } else {
        navigator.clipboard.writeText(mnemonic)
        document.getElementById("backup").innerHTML = "The following key has been copied to clipboard, paste it in the box above and save it in your manager: "
    }

}

async function hash(message) {
	const text_encoder = new TextEncoder;
	const data = text_encoder.encode(message);
	const message_digest = await window.crypto.subtle.digest("SHA-256", data);
	return arr2hex(message_digest);
}

function createMutationObserver() {
    var target = document.querySelector('body'),

        config = {
            attributes: true,
            attributeOldValue: false,
            characterData: true,
            characterDataOldValue: false,
            childList: true,
            subtree: true
        },
        observer = new MutationObserver(function (mutations) {
            parent.postMessage({
                type: "resize",
                data: document.body.scrollHeight
            }, pars.get("callback"));
        });

    observer.observe(target, config);
}

const NETWORK = "https://smartlike.org/network"

function arr2hex(buffer) {
    return [...new Uint8Array(buffer)]
        .map(x => x.toString(16).padStart(2, "0"))
        .join("")
}

function signHex(message, secret) {
    var seed = blakejs.blake2b(new TextEncoder().encode(secret), undefined).slice(0, 32)
    var keys = nacl.sign.keyPair.fromSeed(seed)
    var sig = nacl.sign(new TextEncoder().encode(message), keys.secretKey)
    return [arr2hex(keys.publicKey), arr2hex(sig.subarray(0, nacl.sign.signatureLength))]
}

async function sendTx(message) {
    return fetch(NETWORK, {
            method: "POST",
            headers: {
                'Content-type': 'application/json'
            },
            body: JSON.stringify(message)
        })
        .then((res) => {
            console.log(res)
            if (res.status == 200)
                return res.json()
            else
                return {status: "error", data: "http code " + res.status}
            })
        .catch((err) => {
            return "failed to connect"
        })
}
