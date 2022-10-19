const Utils = {
    plural(n, singular, plural) {
        const noun = n === 1 ? singular : plural
        return `${n} ${noun}`
    },
    pluralS(n, singular) {
        return Utils.plural(n, singular, `${singular}s`)
    },
    formatDuration(seconds) {
        if (seconds < 1) {
            return `${Math.round(seconds * 1e3)}ms`
        } else if (seconds < 60) {
            return `${seconds.toFixed(1)}s`
        } else {
            return `${(seconds / 60).toFixed(1)}min`
        }
    },
}

const appComponent = Vue.createApp({
    template: '#app',
    data() {
        return {
            loaded: false,
            loading: false,
            lastUpdate: new Date,
            name: null,
            columns: [],
        }
    },
    methods: {
        ...Utils,
        async update() {
            if (this.loading) {
                return
            }

            this.loading = true
            try {
                const response = await (await fetch('/api/board')).json()
                this.name = response.name
                this.columns = response.columns
                this.loaded = true
                this.lastUpdate = new Date
            } finally {
                this.loading = false
            }
        },
    }
})

appComponent.component('relative-date', {
    props: ['date'],
    template: '#relative-date',
    data() {
        return {
            interval: null,
            text: '',
        }
    },
    created() {
        this.updateLoop()
    },
    destroyed() {
        clearInterval(this.interval)
    },
    methods: {
        updateLoop() {
            const date = this.date instanceof Date ? this.date : new Date(this.date)
            const deltaSeconds = (Date.now() - date.getTime()) / 1e3
            let tickLength
            if (deltaSeconds < 60) {
                this.text = `${Utils.pluralS(deltaSeconds.toFixed(0), 'second')} ago`
                tickLength = 1e3
            } else if (deltaSeconds < 3600) {
                this.text = `${Utils.pluralS((deltaSeconds / 60).toFixed(1), 'minute')} ago`
                tickLength = 6e3
            } else {
                this.text = `${Utils.pluralS((deltaSeconds / 3600).toFixed(1), 'hour')} ago`
                tickLength = 360e3
            }
            this.interval = setTimeout(() => this.updateLoop(), tickLength)
        }
    }
})

appComponent.component('board-column', {
    props: ['name', 'issues'],
    template: '#board-column',
})

appComponent.component('board-issue', {
    props: ['issueKey', 'jiraLink', 'summary', 'status', 'avatars', 'epic'],
    template: '#board-issue',
})

const app = appComponent.mount('#main')

async function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms))
}

async function mainLoop() {
    while (true) {
        try {
            await app.update()
        } catch (error) {
            console.error(error)
        }

        const is_visible = document.visibilityState !== 'hidden'
        await sleep(is_visible ? 10e3 : 90e3)
    }
}

mainLoop()
