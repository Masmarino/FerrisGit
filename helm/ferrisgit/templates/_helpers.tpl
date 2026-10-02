{{- define "ferrisgit.labels" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ toString (.Values.image.tag | default .Chart.AppVersion) | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end -}}

{{- define "ferrisgit.selectorLabels" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "ferrisgit.image" -}}
{{- $tag := required "image.tag must be set to a release tag (e.g. --set image.tag=1.4.2)" (toString (.Values.image.tag | default "")) -}}
{{- if .Values.image.digest -}}
{{ .Values.image.repository }}:{{ $tag }}@{{ .Values.image.digest }}
{{- else -}}
{{ .Values.image.repository }}:{{ $tag }}
{{- end -}}
{{- end -}}

{{/* A moving tag would never be refreshed under IfNotPresent. */}}
{{- define "ferrisgit.pullPolicy" -}}
{{- if and (eq (toString .Values.image.tag) "latest") (not .Values.image.digest) }}Always{{ else }}{{ .Values.image.pullPolicy }}{{ end -}}
{{- end -}}

{{- define "ferrisgit.publicUrl" -}}
{{- if .Values.ferrisgit.publicUrl -}}
{{ .Values.ferrisgit.publicUrl | trimSuffix "/" }}
{{- else -}}
https://{{ required "ingress.host or ferrisgit.publicUrl must be set" .Values.ingress.host }}
{{- end -}}
{{- end -}}

{{/* Comma-separated CIDRs with the blanks stripped; empty when nothing is left. */}}
{{- define "ferrisgit.trustedProxyCidrs" -}}
{{- $items := list -}}
{{- range splitList "," (toString (default "" .Values.ferrisgit.trustedProxyCidrs)) -}}
{{- with trim . -}}{{- $items = append $items . -}}{{- end -}}
{{- end -}}
{{- join "," $items -}}
{{- end -}}

{{- define "ferrisgit.secretName" -}}
{{ .Release.Name }}-secrets
{{- end -}}

{{- define "ferrisgit.serviceAccountName" -}}
{{ .Release.Name }}-ci
{{- end -}}

{{/* Namespace the CI job Pods run in. */}}
{{- define "ferrisgit.jobsNamespace" -}}
{{ .Values.kubernetesExecutor.namespace | default .Release.Namespace }}
{{- end -}}
