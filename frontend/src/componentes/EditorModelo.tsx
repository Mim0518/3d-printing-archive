import {
  Alert,
  Autocomplete,
  Button,
  Code,
  Group,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  TagsInput,
  Text,
  TextInput,
  Textarea,
} from '@mantine/core'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { api } from '../api'
import { rutaNueva } from '../formato'
import { REVISADO, useOpciones } from '../opciones'
import type { Ficha } from '../tipos'

/** Formulario para editar un modelo. La ruta nueva se calcula igual que en el servidor. */
export default function EditorModelo({ modelo, alTerminar }: { modelo: Ficha; alTerminar: () => void }) {
  const qc = useQueryClient()
  const { filtros, etiquetas } = useOpciones()
  const [f, setF] = useState({
    juego: modelo.juego,
    faccion: modelo.faccion,
    nombre: modelo.nombre,
    estudio: modelo.estudio,
    subcarpeta: modelo.subcarpeta,
    confianza: modelo.confianza,
    notas: modelo.notas,
    revisado: modelo.revisado,
    etiquetas: modelo.etiquetas,
  })
  const poner = (campo: keyof typeof f) => (v: string | string[]) => setF((x) => ({ ...x, [campo]: v }))
  const guardar = useMutation({
    mutationFn: () => api<{ cambios: number }>(`/modelos/${modelo.id}`, { method: 'PATCH', body: JSON.stringify(f) }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['modelo', String(modelo.id)] })
      void qc.invalidateQueries({ queryKey: ['modelos'] })
      void qc.invalidateQueries({ queryKey: ['filtros'] })
      void qc.invalidateQueries({ queryKey: ['etiquetas'] })
      void qc.invalidateQueries({ queryKey: ['historial', modelo.id] })
      alTerminar()
    },
  })

  const ruta = rutaNueva(f.juego.trim(), f.faccion.trim(), f.nombre.trim(), f.estudio.trim(), f.subcarpeta.trim())
  const k = modelo.carpeta.replace(/\/\(sueltos\)$/, '')
  const enSuLugar = k === ruta || k.startsWith(ruta + '/')
  const juegos = filtros?.juegos.map((j) => j.nombre) ?? []
  const facciones = [...new Set((filtros?.facciones ?? []).filter((x) => x.juego === f.juego).map((x) => x.nombre))]
  const estudios = filtros?.estudios.map((e) => e.nombre) ?? []

  return (
    <Stack>
      <SimpleGrid cols={{ base: 1, sm: 2 }}>
        <Autocomplete label="Juego" data={juegos} value={f.juego} onChange={poner('juego')} />
        <Autocomplete label="Facción / tipo" data={facciones} value={f.faccion} onChange={poner('faccion')} />
        <TextInput
          label="Modelo"
          description="Nombre oficial en inglés; los bits como «Bits/<nombre>»"
          value={f.nombre}
          onChange={(e) => poner('nombre')(e.currentTarget.value)}
        />
        <Autocomplete label="Estudio" data={estudios} value={f.estudio} onChange={poner('estudio')} />
        <TextInput
          label="Subcarpeta"
          description="Solo si dos filas caen en la misma carpeta"
          value={f.subcarpeta}
          onChange={(e) => poner('subcarpeta')(e.currentTarget.value)}
        />
        <Select
          label="Confianza"
          data={['alta', 'media', 'baja']}
          clearable
          value={f.confianza || null}
          onChange={(v) => poner('confianza')(v ?? '')}
        />
      </SimpleGrid>
      <TagsInput
        label="Etiquetas"
        data={etiquetas.map((e) => e.nombre)}
        value={f.etiquetas}
        onChange={poner('etiquetas')}
        clearable
      />
      <Textarea label="Notas" autosize minRows={2} value={f.notas} onChange={(e) => poner('notas')(e.currentTarget.value)} />
      <div>
        <Text size="sm" fw={500} mb={4}>
          Revisado
        </Text>
        <SegmentedControl data={REVISADO} value={f.revisado} onChange={poner('revisado')} />
      </div>
      <div>
        <Text size="xs" c="dimmed">
          Ruta nueva
        </Text>
        <Code block>{ruta}</Code>
        {!enSuLugar && f.revisado !== 'No mover' && (
          <Text size="xs" c="orange" mt={4}>
            Al guardar queda pendiente de mover; la mudanza se hace aparte, con plan y opción de deshacer.
          </Text>
        )}
      </div>
      {guardar.error && (
        <Alert color="red" variant="light">
          {guardar.error.message}
        </Alert>
      )}
      <Group justify="flex-end">
        <Button variant="default" onClick={alTerminar}>
          Cancelar
        </Button>
        <Button onClick={() => guardar.mutate()} loading={guardar.isPending}>
          Guardar
        </Button>
      </Group>
    </Stack>
  )
}
