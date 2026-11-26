import { Alert, Autocomplete, Button, Group, Select, Stack, TagsInput, Text } from '@mantine/core'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { api } from '../api'
import { REVISADO, useOpciones } from '../opciones'

/** Cambiar varios modelos a la vez: solo se aplican los campos que se llenan. */
export default function EdicionLote({ ids, alTerminar }: { ids: number[]; alTerminar: () => void }) {
  const qc = useQueryClient()
  const { filtros, etiquetas } = useOpciones()
  const [juego, setJuego] = useState('')
  const [faccion, setFaccion] = useState('')
  const [estudio, setEstudio] = useState('')
  const [confianza, setConfianza] = useState<string | null>(null)
  const [revisado, setRevisado] = useState<string | null>(null)
  const [agregar, setAgregar] = useState<string[]>([])
  const [quitar, setQuitar] = useState<string[]>([])

  const guardar = useMutation({
    mutationFn: () => {
      const cambios: Record<string, string> = {}
      if (juego.trim()) cambios.juego = juego
      if (faccion.trim()) cambios.faccion = faccion
      if (estudio.trim()) cambios.estudio = estudio
      if (confianza !== null) cambios.confianza = confianza
      if (revisado !== null) cambios.revisado = revisado
      return api<{ modelos: number; cambios: number }>('/modelos/lote', {
        method: 'PATCH',
        body: JSON.stringify({ ids, cambios, agregar_etiquetas: agregar, quitar_etiquetas: quitar }),
      })
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['modelos'] })
      void qc.invalidateQueries({ queryKey: ['filtros'] })
      void qc.invalidateQueries({ queryKey: ['etiquetas'] })
      void qc.invalidateQueries({ queryKey: ['modelo'] })
      alTerminar()
    },
  })

  const nombresEt = etiquetas.map((e) => e.nombre)
  const facciones = [
    ...new Set((filtros?.facciones ?? []).filter((x) => !juego || x.juego === juego).map((x) => x.nombre)),
  ]
  const nada = !juego.trim() && !faccion.trim() && !estudio.trim() && confianza === null && revisado === null &&
    agregar.length === 0 && quitar.length === 0

  return (
    <Stack>
      <Text size="sm" c="dimmed">
        {ids.length} modelos seleccionados. Solo cambian los campos que llenes; cada cambio queda en su historial.
      </Text>
      <Autocomplete label="Juego" data={filtros?.juegos.map((j) => j.nombre) ?? []} value={juego} onChange={setJuego} />
      <Autocomplete label="Facción / tipo" data={facciones} value={faccion} onChange={setFaccion} />
      <Autocomplete label="Estudio" data={filtros?.estudios.map((e) => e.nombre) ?? []} value={estudio} onChange={setEstudio} />
      <Select label="Confianza" data={['alta', 'media', 'baja']} clearable value={confianza} onChange={setConfianza} />
      <Select
        label="Revisado"
        data={REVISADO.map((r) => ({ value: r.value || 'vacio', label: r.label }))}
        clearable
        value={revisado === '' ? 'vacio' : revisado}
        onChange={(v) => setRevisado(v === null ? null : v === 'vacio' ? '' : v)}
      />
      <TagsInput label="Agregar etiquetas" data={nombresEt} value={agregar} onChange={setAgregar} clearable />
      <TagsInput label="Quitar etiquetas" data={nombresEt} value={quitar} onChange={setQuitar} clearable />
      {guardar.error && (
        <Alert color="red" variant="light">
          {guardar.error.message}
        </Alert>
      )}
      <Group justify="flex-end">
        <Button variant="default" onClick={alTerminar}>
          Cancelar
        </Button>
        <Button onClick={() => guardar.mutate()} loading={guardar.isPending} disabled={nada}>
          Aplicar a {ids.length}
        </Button>
      </Group>
    </Stack>
  )
}
